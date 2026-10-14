//! A signed-in Airthings session: keeps a valid access token and the account ID for API calls.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::net::TcpListener;

use crate::api::{ApiClient, RemoteControlState};
use crate::auth::{self, AuthClient, Credentials, Pkce, TokenResponse, DEFAULT_AUTHORIZE_URL};
use crate::error::{Error, Result};
use crate::secrets::SecretStore;

/// Renew tokens this long before they expire.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);
/// Assumed token lifetime when Airthings does not say.
const DEFAULT_LIFETIME: Duration = Duration::from_secs(3600);
/// How long to wait for the user to finish signing in in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

struct Token {
    access_token: String,
    expires_at: Instant,
}

impl Token {
    fn from_response(resp: &TokenResponse) -> Self {
        let lifetime = resp.expires_in.map(Duration::from_secs).unwrap_or(DEFAULT_LIFETIME);
        Token {
            access_token: resp.access_token.clone(),
            expires_at: Instant::now() + lifetime.saturating_sub(EXPIRY_MARGIN),
        }
    }

    fn is_fresh(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

pub struct Session {
    auth: AuthClient,
    api: ApiClient,
    secrets: Arc<dyn SecretStore>,
    credentials: Mutex<Option<Credentials>>,
    // Async mutex so concurrent callers wait for a single token refresh.
    token: tokio::sync::Mutex<Option<Token>>,
    account_id: Mutex<Option<String>>,
}

impl Session {
    pub fn new(auth: AuthClient, api: ApiClient, secrets: Arc<dyn SecretStore>) -> Self {
        let credentials = secrets.load();
        Session {
            auth,
            api,
            secrets,
            credentials: Mutex::new(credentials),
            token: tokio::sync::Mutex::new(None),
            account_id: Mutex::new(None),
        }
    }

    pub fn auth(&self) -> &AuthClient {
        &self.auth
    }

    pub fn api(&self) -> &ApiClient {
        &self.api
    }

    pub fn credentials(&self) -> Option<Credentials> {
        self.credentials.lock().unwrap().clone()
    }

    pub fn is_signed_in(&self) -> bool {
        self.credentials.lock().unwrap().is_some()
    }

    /// Sign in with client credentials: verifies them by fetching a token before saving.
    pub async fn sign_in(&self, credentials: Credentials) -> Result<()> {
        let resp = self.auth.token_for(&credentials).await?;
        self.complete_sign_in(credentials, &resp)
    }

    /// Finish a sign-in whose first token was already obtained (the OAuth code exchange).
    pub fn complete_sign_in(&self, credentials: Credentials, resp: &TokenResponse) -> Result<()> {
        let credentials = rotate(credentials, resp);
        self.secrets.save(&credentials)?;
        *self.credentials.lock().unwrap() = Some(credentials);
        *self.account_id.lock().unwrap() = None;
        *self.token.try_lock().map_err(|_| Error::Other("sign-in already in progress".into()))? = Some(Token::from_response(resp));
        Ok(())
    }

    /// Sign in through the Airthings login page: `open_browser` is handed the authorize URL, and the
    /// result comes back to a loopback listener on `redirect_port`.
    pub async fn sign_in_oauth(
        &self,
        client_id: String,
        client_secret: Option<String>,
        redirect_port: u16,
        open_browser: impl FnOnce(String) -> Result<()>,
    ) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", redirect_port))
            .await
            .map_err(|e| Error::Other(format!("could not listen for the sign-in redirect on port {redirect_port}: {e}")))?;

        let pkce = Pkce::generate();
        let csrf_state = auth::random_token();
        let redirect_uri = auth::redirect_uri(redirect_port);
        open_browser(auth::authorize_url(DEFAULT_AUTHORIZE_URL, &client_id, &redirect_uri, &csrf_state, &pkce.challenge)?)?;

        let code = tokio::time::timeout(SIGN_IN_TIMEOUT, auth::wait_for_code(listener, &csrf_state))
            .await
            .map_err(|_| Error::Other("timed out waiting for the Airthings sign-in".into()))??;
        let resp = self
            .auth
            .exchange_code(&client_id, client_secret.as_deref(), &code, &redirect_uri, &pkce.verifier)
            .await?;
        let refresh_token = resp
            .refresh_token
            .clone()
            .ok_or_else(|| Error::Other("Airthings did not return a refresh token".into()))?;
        self.complete_sign_in(Credentials::RefreshToken { client_id, client_secret, refresh_token }, &resp)
    }

    pub async fn sign_out(&self) {
        self.secrets.clear();
        *self.credentials.lock().unwrap() = None;
        *self.account_id.lock().unwrap() = None;
        *self.token.lock().await = None;
    }

    pub async fn access_token(&self) -> Result<String> {
        let mut token = self.token.lock().await;
        if let Some(t) = token.as_ref().filter(|t| t.is_fresh()) {
            return Ok(t.access_token.clone());
        }
        let credentials = self.credentials().ok_or(Error::Unauthorized)?;
        let resp = match self.auth.token_for(&credentials).await {
            Ok(resp) => resp,
            // A revoked refresh token or deleted API client means the user has to sign in again.
            Err(Error::Rejected(_)) => {
                drop(token);
                self.sign_out().await;
                return Err(Error::Unauthorized);
            }
            Err(e) => return Err(e),
        };
        let rotated = rotate(credentials.clone(), &resp);
        if rotated != credentials {
            self.secrets.save(&rotated)?;
            *self.credentials.lock().unwrap() = Some(rotated);
        }
        let fresh = Token::from_response(&resp);
        let access_token = fresh.access_token.clone();
        *token = Some(fresh);
        Ok(access_token)
    }

    async fn invalidate_token(&self) {
        *self.token.lock().await = None;
    }

    /// Run an API call with a valid token, retrying once with a new token if Airthings answers 401.
    pub async fn call<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let token = self.access_token().await?;
        match f(token).await {
            Err(Error::Unauthorized) => {
                self.invalidate_token().await;
                let token = self.access_token().await?;
                f(token).await
            }
            other => other,
        }
    }

    /// The account to read devices from. Consumer users have exactly one.
    pub async fn account_id(&self) -> Result<String> {
        if let Some(id) = self.account_id.lock().unwrap().clone() {
            return Ok(id);
        }
        let accounts = self.call(|t| async move { self.api.accounts(&t).await }).await?;
        let id = accounts
            .into_iter()
            .next()
            .map(|a| a.id)
            .ok_or_else(|| Error::Other("this Airthings user has no accounts".into()))?;
        *self.account_id.lock().unwrap() = Some(id.clone());
        Ok(id)
    }

    /// Last mode reported by a Renew air purifier, or `None` if it has never synced one.
    pub async fn remote_control(&self, serial: &str) -> Result<Option<RemoteControlState>> {
        let account = self.account_id().await?;
        let account = account.as_str();
        self.call(|t| async move { self.api.remote_control(&t, account, serial).await }).await
    }

    /// Queue a mode change for a Renew air purifier. It applies the next time the device syncs.
    pub async fn set_remote_control(&self, serial: &str, state: &RemoteControlState) -> Result<()> {
        let account = self.account_id().await?;
        let account = account.as_str();
        self.call(|t| async move { self.api.set_remote_control(&t, account, serial, state).await }).await
    }
}

/// Airthings may hand out a new refresh token with each refresh; keep the latest one.
fn rotate(credentials: Credentials, resp: &TokenResponse) -> Credentials {
    match (credentials, &resp.refresh_token) {
        (Credentials::RefreshToken { client_id, client_secret, .. }, Some(new)) => {
            Credentials::RefreshToken { client_id, client_secret, refresh_token: new.clone() }
        }
        (credentials, _) => credentials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn session(server: &MockServer, secrets: Arc<dyn SecretStore>) -> Session {
        let http = reqwest::Client::new();
        Session::new(
            AuthClient::new(http.clone(), format!("{}/v1/token", server.uri())),
            ApiClient::new(http, server.uri()),
            secrets,
        )
    }

    fn client_credentials() -> Credentials {
        Credentials::ClientCredentials { client_id: "id".into(), client_secret: "s".into(), scope: "read".into() }
    }

    async fn token_endpoint(server: &MockServer, access_token: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": access_token, "expires_in": 3600})))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn sign_in_saves_credentials_and_reuses_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a", "expires_in": 3600})))
            .expect(1)
            .mount(&server)
            .await;
        let secrets = Arc::new(MemoryStore::default());
        let session = session(&server, secrets.clone());

        session.sign_in(client_credentials()).await.unwrap();
        assert_eq!(secrets.load(), Some(client_credentials()));
        assert_eq!(session.access_token().await.unwrap(), "a");
        assert_eq!(session.access_token().await.unwrap(), "a");
    }

    #[tokio::test]
    async fn restores_saved_credentials_on_start() {
        let server = MockServer::start().await;
        token_endpoint(&server, "restored").await;
        let secrets = Arc::new(MemoryStore::default());
        secrets.save(&client_credentials()).unwrap();

        let session = session(&server, secrets);
        assert!(session.is_signed_in());
        assert_eq!(session.access_token().await.unwrap(), "restored");
    }

    #[tokio::test]
    async fn retries_once_with_new_token_after_401() {
        let server = MockServer::start().await;
        token_endpoint(&server, "new").await;
        Mock::given(path("/v1/accounts"))
            .and(header("authorization", "Bearer new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accounts": [{"id": "acc"}]})))
            .mount(&server)
            .await;
        Mock::given(path("/v1/accounts")).respond_with(ResponseTemplate::new(401)).mount(&server).await;

        let session = session(&server, Arc::new(MemoryStore::default()));
        session.complete_sign_in(client_credentials(), &TokenResponse { access_token: "stale".into(), expires_in: Some(3600), refresh_token: None }).unwrap();

        assert_eq!(session.account_id().await.unwrap(), "acc");
    }

    #[tokio::test]
    async fn keeps_rotated_refresh_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(json!({"grant_type": "refresh_token", "refresh_token": "r1"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a2", "refresh_token": "r2", "expires_in": 3600})))
            .mount(&server)
            .await;
        let secrets = Arc::new(MemoryStore::default());
        let session = session(&server, secrets.clone());
        let creds = Credentials::RefreshToken { client_id: "id".into(), client_secret: None, refresh_token: "r0".into() };
        // The code exchange already returned r1 and a token that is about to expire.
        session.complete_sign_in(creds, &TokenResponse { access_token: "a1".into(), expires_in: Some(0), refresh_token: Some("r1".into()) }).unwrap();

        assert_eq!(session.access_token().await.unwrap(), "a2");
        assert!(matches!(secrets.load(), Some(Credentials::RefreshToken { refresh_token, .. }) if refresh_token == "r2"));
    }

    #[tokio::test]
    async fn rejected_refresh_signs_out() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})))
            .mount(&server)
            .await;
        let secrets = Arc::new(MemoryStore::default());
        secrets.save(&client_credentials()).unwrap();
        let session = session(&server, secrets.clone());

        assert!(matches!(session.access_token().await, Err(Error::Unauthorized)));
        assert!(!session.is_signed_in());
        assert_eq!(secrets.load(), None);
    }

    #[tokio::test]
    async fn oauth_sign_in_exchanges_the_code_from_the_browser_redirect() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(json!({"grant_type": "authorization_code", "code": "xyz"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a", "refresh_token": "r", "expires_in": 3600})))
            .mount(&server)
            .await;
        let secrets = Arc::new(MemoryStore::default());
        let session = session(&server, secrets.clone());
        let port = 53999;

        // Stand in for the browser: follow the authorize URL's state back to the loopback redirect.
        let browser = |url: String| {
            let url = reqwest::Url::parse(&url).unwrap();
            let state = url.query_pairs().find(|(k, _)| k == "state").unwrap().1.into_owned();
            tokio::spawn(reqwest::get(format!("{}?code=xyz&state={state}", auth::redirect_uri(port))));
            Ok(())
        };
        session.sign_in_oauth("id".into(), None, port, browser).await.unwrap();

        assert!(matches!(secrets.load(), Some(Credentials::RefreshToken { refresh_token, .. }) if refresh_token == "r"));
        assert_eq!(session.access_token().await.unwrap(), "a");
    }

    #[tokio::test]
    async fn signed_out_session_is_unauthorized() {
        let server = MockServer::start().await;
        let session = session(&server, Arc::new(MemoryStore::default()));
        assert!(matches!(session.access_token().await, Err(Error::Unauthorized)));
    }
}
