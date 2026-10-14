//! Airthings account authentication: OAuth authorization code (with PKCE and a loopback redirect)
//! and the client-credentials fallback for users who bring their own API client.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::error::{Error, Result};

pub const DEFAULT_TOKEN_URL: &str = "https://accounts-api.airthings.com/v1/token";
pub const DEFAULT_AUTHORIZE_URL: &str = "https://accounts.airthings.com/authorize";
pub const DEFAULT_SCOPE: &str = "read:device:current_values";
pub const DEFAULT_REDIRECT_PORT: u16 = 53682;
pub const CALLBACK_PATH: &str = "/callback";

/// OAuth client compiled into release builds. Empty when the build has none registered.
pub const BUILTIN_CLIENT_ID: Option<&str> = option_env!("AIRDASH_CLIENT_ID");
pub const BUILTIN_CLIENT_SECRET: Option<&str> = option_env!("AIRDASH_CLIENT_SECRET");

/// What the app keeps (in the OS keychain) to get new access tokens without asking the user again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Credentials {
    #[serde(rename_all = "camelCase")]
    ClientCredentials { client_id: String, client_secret: String, scope: String },
    #[serde(rename_all = "camelCase")]
    RefreshToken { client_id: String, client_secret: Option<String>, refresh_token: String },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Clone)]
pub struct AuthClient {
    http: reqwest::Client,
    token_url: String,
}

impl AuthClient {
    pub fn new(http: reqwest::Client, token_url: impl Into<String>) -> Self {
        AuthClient { http, token_url: token_url.into() }
    }

    pub async fn client_credentials(&self, client_id: &str, client_secret: &str, scope: &str) -> Result<TokenResponse> {
        let scopes: Vec<&str> = scope.split_whitespace().collect();
        self.request(json!({
            "grant_type": "client_credentials",
            "client_id": client_id,
            "client_secret": client_secret,
            "scope": scopes,
        }))
        .await
    }

    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<TokenResponse> {
        let mut body = json!({
            "grant_type": "authorization_code",
            "client_id": client_id,
            "code": code,
            "redirect_uri": redirect_uri,
            "code_verifier": code_verifier,
        });
        if let Some(secret) = client_secret {
            body["client_secret"] = json!(secret);
        }
        self.request(body).await
    }

    pub async fn refresh(&self, client_id: &str, client_secret: Option<&str>, refresh_token: &str) -> Result<TokenResponse> {
        let mut body = json!({
            "grant_type": "refresh_token",
            "client_id": client_id,
            "refresh_token": refresh_token,
        });
        if let Some(secret) = client_secret {
            body["client_secret"] = json!(secret);
        }
        self.request(body).await
    }

    /// Get a fresh access token for stored credentials. A rotated refresh token is returned in the response.
    pub async fn token_for(&self, credentials: &Credentials) -> Result<TokenResponse> {
        match credentials {
            Credentials::ClientCredentials { client_id, client_secret, scope } => {
                self.client_credentials(client_id, client_secret, scope).await
            }
            Credentials::RefreshToken { client_id, client_secret, refresh_token } => {
                self.refresh(client_id, client_secret.as_deref(), refresh_token).await
            }
        }
    }

    async fn request(&self, body: Value) -> Result<TokenResponse> {
        let resp = self.http.post(&self.token_url).json(&body).send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json().await?);
        }
        let text = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                ["error_description", "message", "error"]
                    .iter()
                    .find_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_owned))
            })
            .unwrap_or(text);
        if status.as_u16() == 400 || status.as_u16() == 401 {
            return Err(Error::Other(format!("Airthings rejected the sign-in: {message}")));
        }
        Err(Error::Api { status: status.as_u16(), message })
    }
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Self {
        let verifier = random_token();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Pkce { verifier, challenge }
    }
}

/// 256 bits of randomness, base64url encoded (43 characters).
pub fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{CALLBACK_PATH}")
}

pub fn authorize_url(base: &str, client_id: &str, redirect_uri: &str, state: &str, code_challenge: &str) -> Result<String> {
    let url = Url::parse_with_params(
        base,
        &[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect_uri),
            ("state", state),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
        ],
    )
    .map_err(|e| Error::Other(format!("invalid authorize URL: {e}")))?;
    Ok(url.into())
}

#[derive(Debug, PartialEq)]
pub enum Callback {
    /// A request for something other than the callback, such as `/favicon.ico`.
    Ignored,
    Code(String),
    Denied(String),
}

/// Interpret the request line the browser sends to the loopback listener.
pub fn parse_callback(request_line: &str, expected_state: &str) -> Result<Callback> {
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let url = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|e| Error::Other(e.to_string()))?;
    if url.path() != CALLBACK_PATH {
        return Ok(Callback::Ignored);
    }
    let param = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned());
    if let Some(error) = param("error") {
        return Ok(Callback::Denied(param("error_description").unwrap_or(error)));
    }
    if param("state").as_deref() != Some(expected_state) {
        return Err(Error::Other("sign-in response did not match this request (state mismatch)".into()));
    }
    param("code")
        .map(Callback::Code)
        .ok_or_else(|| Error::Other("sign-in response had no authorization code".into()))
}

/// Serve the loopback redirect until the browser delivers the authorization code.
pub async fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut socket, _) = listener.accept().await.map_err(|e| Error::Other(e.to_string()))?;
        let mut buf = vec![0u8; 8192];
        let n = socket.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let first_line = request.lines().next().unwrap_or_default();

        let outcome = parse_callback(first_line, expected_state);
        let (status, message) = match &outcome {
            Ok(Callback::Ignored) => ("404 Not Found", "Not found"),
            Ok(Callback::Code(_)) => ("200 OK", "You are signed in to AirDash. You can close this tab."),
            Ok(Callback::Denied(_)) => ("200 OK", "Sign-in was cancelled. You can close this tab."),
            Err(_) => ("400 Bad Request", "Sign-in failed. Return to AirDash and try again."),
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>AirDash</title>\
             <body style=\"font-family:system-ui;text-align:center;padding:4rem\"><h2>{message}</h2></body>"
        );
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;

        match outcome? {
            Callback::Ignored => continue,
            Callback::Code(code) => return Ok(code),
            Callback::Denied(reason) => return Err(Error::Other(format!("sign-in was cancelled: {reason}"))),
        }
    }
}
