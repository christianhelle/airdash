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
/// Below Windows' dynamic port range (49152 and up), where Hyper-V, WSL and Docker reserve blocks of ports.
pub const DEFAULT_REDIRECT_PORT: u16 = 43682;
pub const CALLBACK_PATH: &str = "/callback";

/// What to keep between runs to get new access tokens without asking the user again.
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
            return Err(Error::Rejected(message));
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
            Ok(Callback::Code(_)) => ("200 OK", "You are signed in to Airthings. You can close this tab."),
            Ok(Callback::Denied(_)) => ("200 OK", "Sign-in was cancelled. You can close this tab."),
            Err(_) => ("400 Bad Request", "Sign-in failed. Return to the app and try again."),
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Airthings sign-in</title>\
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn client_credentials_sends_scope_list() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .and(body_json(json!({
                "grant_type": "client_credentials", "client_id": "id", "client_secret": "secret",
                "scope": ["read:device:current_values"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a", "expires_in": 10800, "token_type": "Bearer"})))
            .mount(&server)
            .await;

        let auth = AuthClient::new(reqwest::Client::new(), format!("{}/v1/token", server.uri()));
        let token = auth.client_credentials("id", "secret", DEFAULT_SCOPE).await.unwrap();
        assert_eq!(token, TokenResponse { access_token: "a".into(), expires_in: Some(10800), refresh_token: None });
    }

    #[tokio::test]
    async fn exchanges_code_with_verifier_and_without_secret_for_public_clients() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(json!({
                "grant_type": "authorization_code", "client_id": "id", "code": "c",
                "redirect_uri": "http://127.0.0.1:43682/callback", "code_verifier": "v"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600})))
            .mount(&server)
            .await;

        let auth = AuthClient::new(reqwest::Client::new(), server.uri());
        let token = auth.exchange_code("id", None, "c", &redirect_uri(43682), "v").await.unwrap();
        assert_eq!(token.refresh_token.as_deref(), Some("r"));
    }

    #[tokio::test]
    async fn refresh_includes_secret_when_present() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(json!({"grant_type": "refresh_token", "client_id": "id", "client_secret": "s", "refresh_token": "r"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a2"})))
            .mount(&server)
            .await;

        let auth = AuthClient::new(reqwest::Client::new(), server.uri());
        let creds = Credentials::RefreshToken { client_id: "id".into(), client_secret: Some("s".into()), refresh_token: "r".into() };
        assert_eq!(auth.token_for(&creds).await.unwrap().access_token, "a2");
    }

    #[tokio::test]
    async fn rejected_sign_in_reports_airthings_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid_client", "error_description": "Bad client"})))
            .mount(&server)
            .await;

        let auth = AuthClient::new(reqwest::Client::new(), server.uri());
        let err = auth.client_credentials("id", "wrong", DEFAULT_SCOPE).await.unwrap_err();
        assert_eq!(err.to_string(), "Airthings rejected the sign-in: Bad client");
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        // Example from RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");

        let pkce = Pkce::generate();
        assert_eq!(pkce.verifier.len(), 43);
        assert_ne!(pkce.verifier, Pkce::generate().verifier);
    }

    #[test]
    fn authorize_url_carries_all_parameters() {
        let url = authorize_url(DEFAULT_AUTHORIZE_URL, "my client", &redirect_uri(43682), "st", "ch").unwrap();
        assert_eq!(
            url,
            "https://accounts.airthings.com/authorize?response_type=code&client_id=my+client\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A43682%2Fcallback&state=st&code_challenge=ch&code_challenge_method=S256"
        );
    }

    #[test]
    fn parses_callback_requests() {
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1", "s").unwrap(), Callback::Ignored);
        assert_eq!(parse_callback("GET /callback?code=abc&state=s HTTP/1.1", "s").unwrap(), Callback::Code("abc".into()));
        assert_eq!(
            parse_callback("GET /callback?error=access_denied&state=s HTTP/1.1", "s").unwrap(),
            Callback::Denied("access_denied".into())
        );
        assert!(parse_callback("GET /callback?code=abc&state=other HTTP/1.1", "s").is_err());
        assert!(parse_callback("GET /callback?state=s HTTP/1.1", "s").is_err());
    }

    #[tokio::test]
    async fn loopback_listener_returns_code_from_browser_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move { wait_for_code(listener, "st").await });

        let http = reqwest::Client::new();
        let favicon = http.get(format!("http://127.0.0.1:{port}/favicon.ico")).send().await.unwrap();
        assert_eq!(favicon.status(), 404);
        let page = http.get(format!("{}?code=xyz&state=st", redirect_uri(port))).send().await.unwrap();
        assert!(page.text().await.unwrap().contains("signed in"));

        assert_eq!(server.await.unwrap().unwrap(), "xyz");
    }
}
