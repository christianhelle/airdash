# airthings-client

A Rust client for the [Airthings Consumer API](https://consumer-api-doc.airthings.com/): sign in, keep a valid
access token and read your devices and their latest sensor values.

- Client-credentials sign-in, or OAuth authorization code with PKCE through the user's browser and a loopback
  redirect
- Access tokens are cached and refreshed before they expire, rotated refresh tokens are kept, and a call that
  gets a 401 is retried once with a new token
- Pluggable credential storage through the `SecretStore` trait, with an in-memory store included
- Rate-limit headers and `429` responses are surfaced as typed values
- Async, built on [`reqwest`](https://crates.io/crates/reqwest) and [`tokio`](https://crates.io/crates/tokio)

This crate is not affiliated with or endorsed by Airthings.

## Contents

- [Installation](#installation)
- [Getting API credentials](#getting-api-credentials)
- [Quick start](#quick-start)
- [How the crate is organized](#how-the-crate-is-organized)
- [Signing in](#signing-in)
- [Reading devices and sensors](#reading-devices-and-sensors)
- [Making API calls through a session](#making-api-calls-through-a-session)
- [Storing credentials between runs](#storing-credentials-between-runs)
- [Handling errors](#handling-errors)
- [Using the clients without a session](#using-the-clients-without-a-session)
- [Testing against a mock server](#testing-against-a-mock-server)
- [Defaults](#defaults)

## Installation

```sh
cargo add airthings-client
cargo add tokio --features macros,rt-multi-thread
cargo add reqwest@0.13 --no-default-features --features rustls
```

You create the `reqwest::Client` the crate uses, so your `reqwest` must be the same major version as this
crate's (currently 0.13). Sharing one `reqwest::Client` lets you configure proxies, timeouts and TLS in one
place.

## Getting API credentials

Create an API client under [Airthings My Applications](https://consumer-api-doc.airthings.com/dashboard) to get
a **client ID** and a **client secret**.

Keep the secret out of source control. The examples below read it from environment variables.

## Quick start

Sign in with client credentials and print the latest value of every sensor on every device:

```rust,no_run
use std::sync::Arc;

use airthings_client::api::{ApiClient, Unit, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, Credentials, DEFAULT_SCOPE, DEFAULT_TOKEN_URL};
use airthings_client::session::Session;
use airthings_client::store::MemoryStore;

#[tokio::main]
async fn main() -> airthings_client::error::Result<()> {
    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        Arc::new(MemoryStore::default()),
    );
    session
        .sign_in(Credentials::ClientCredentials {
            client_id: std::env::var("AIRTHINGS_CLIENT_ID").expect("AIRTHINGS_CLIENT_ID"),
            client_secret: std::env::var("AIRTHINGS_CLIENT_SECRET").expect("AIRTHINGS_CLIENT_SECRET"),
            scope: DEFAULT_SCOPE.into(),
        })
        .await?;

    let account = session.account_id().await?;
    let (session, account) = (&session, account.as_str());
    let (readings, _rate_limit) =
        session.call(|t| async move { session.api().sensors(&t, account, Unit::Metric).await }).await?;
    for device in readings {
        for sensor in device.sensors {
            println!("{} {}: {} {}", device.serial_number, sensor.sensor_type, sensor.value, sensor.unit);
        }
    }
    Ok(())
}
```

## How the crate is organized

| Module    | What it holds                                                                                   |
|-----------|-------------------------------------------------------------------------------------------------|
| `session` | `Session`: signs in, keeps a valid access token, caches the account ID and retries on 401       |
| `auth`    | `AuthClient` for the token endpoint, `Credentials`, PKCE helpers and the loopback redirect      |
| `api`     | `ApiClient` for the Consumer API endpoints and the response types (`Device`, `DeviceSensors`, …) |
| `store`   | The `SecretStore` trait and `MemoryStore`                                                       |
| `error`   | `Error` and `Result`                                                                            |

Most applications only need a `Session`. It wraps an `AuthClient`, an `ApiClient` and a `SecretStore`, and you
reach the API through `session.call(...)` and `session.api()`.

## Signing in

A `Session` is signed in when it holds `Credentials`. There are two kinds:

- `Credentials::ClientCredentials`: your own API client's ID, secret and scope. Each new access token comes
  from the client-credentials grant.
- `Credentials::RefreshToken`: the result of a browser (OAuth) sign-in. Each new access token comes from the
  refresh token, and when Airthings rotates the refresh token, the session saves the new one.

When a session is created, it loads any credentials already in its `SecretStore`, so a saved sign-in carries
over to the next run.

### Client credentials

`Session::sign_in` fetches a token right away, so wrong credentials fail here and are not saved:

```rust,no_run
use std::sync::Arc;

use airthings_client::api::{ApiClient, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, Credentials, DEFAULT_SCOPE, DEFAULT_TOKEN_URL};
use airthings_client::error::Error;
use airthings_client::session::Session;
use airthings_client::store::MemoryStore;

#[tokio::main]
async fn main() {
    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        Arc::new(MemoryStore::default()),
    );
    let credentials = Credentials::ClientCredentials {
        client_id: "your-client-id".into(),
        client_secret: "your-client-secret".into(),
        scope: DEFAULT_SCOPE.into(),
    };
    match session.sign_in(credentials).await {
        Ok(()) => println!("signed in"),
        Err(Error::Rejected(reason)) => eprintln!("Airthings rejected the credentials: {reason}"),
        Err(e) => eprintln!("could not sign in: {e}"),
    }
}
```

### Browser sign-in (OAuth with PKCE)

`Session::sign_in_oauth` runs the authorization code flow:

1. It listens on `127.0.0.1:<redirect_port>`.
2. It builds the Airthings authorize URL, with a PKCE challenge and a random `state`, and hands it to your
   `open_browser` callback.
3. It waits up to five minutes for the browser to come back to `http://127.0.0.1:<redirect_port>/callback`.
4. It exchanges the code for tokens and saves `Credentials::RefreshToken`.

The redirect URI registered on your API client must match `auth::redirect_uri(redirect_port)`, for example
`http://127.0.0.1:43682/callback` with the default port. Pass `None` as the secret for a public client.

```rust,no_run
use std::sync::Arc;

use airthings_client::api::{ApiClient, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, DEFAULT_REDIRECT_PORT, DEFAULT_TOKEN_URL};
use airthings_client::session::Session;
use airthings_client::store::MemoryStore;

#[tokio::main]
async fn main() -> airthings_client::error::Result<()> {
    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        Arc::new(MemoryStore::default()),
    );

    session
        .sign_in_oauth("your-client-id".into(), None, DEFAULT_REDIRECT_PORT, |url| {
            // Open the URL with a crate such as `open` or `webbrowser`, or let the user copy it.
            println!("Open this page to sign in to Airthings:\n{url}");
            Ok(())
        })
        .await?;

    println!("signed in: {}", session.is_signed_in());
    Ok(())
}
```

The listener answers the browser with a short "you can close this tab" page. Requests for other paths, such as
`/favicon.ico`, get a 404 and the listener keeps waiting. A `state` that does not match, or a denied consent,
ends the sign-in with an error.

### Signing out

`session.sign_out().await` clears the `SecretStore` and forgets the token and account ID. A session also signs
itself out when Airthings rejects its stored credentials (a revoked refresh token or a deleted API client), and
the call that found out returns `Error::Unauthorized`.

## Reading devices and sensors

All endpoints are scoped to an account. Consumer users have exactly one, and `session.account_id()` fetches it
once and caches it.

```rust,no_run
use airthings_client::api::Unit;
use airthings_client::session::Session;

async fn print_home(session: &Session) -> airthings_client::error::Result<()> {
    let account = session.account_id().await?;
    let account = account.as_str();

    // Every device on the account, with the sensors it has.
    let devices = session.call(|t| async move { session.api().devices(&t, account).await }).await?;
    for device in &devices {
        println!(
            "{} ({}, {}) in {}: {:?}",
            device.name,
            device.device_type,
            device.serial_number,
            device.home.as_deref().unwrap_or("no home"),
            device.sensors,
        );
    }

    // The latest values for every device. Pages are followed for you.
    let (readings, rate_limit) =
        session.call(|t| async move { session.api().sensors(&t, account, Unit::Imperial).await }).await?;
    for device in &readings {
        println!(
            "{} recorded {} battery {:?}%",
            device.serial_number,
            device.recorded.as_deref().unwrap_or("-"),
            device.battery_percentage,
        );
        for sensor in &device.sensors {
            println!("  {}: {} {}", sensor.sensor_type, sensor.value, sensor.unit);
        }
    }
    println!("{:?} of {:?} requests left, resets {:?}", rate_limit.remaining, rate_limit.limit, rate_limit.reset);
    Ok(())
}
```

| Method                                 | Endpoint                               | Returns                               |
|----------------------------------------|----------------------------------------|---------------------------------------|
| `accounts(token)`                      | `GET /v1/accounts`                     | `Vec<Account>`                        |
| `devices(token, account_id)`           | `GET /v1/accounts/{id}/devices`        | `Vec<Device>`                         |
| `sensors(token, account_id, unit)`     | `GET /v1/accounts/{id}/sensors`        | `(Vec<DeviceSensors>, RateLimit)`     |

Notes on the data:

- `sensor_type` and `unit` are the strings Airthings sends, such as `radonShortTermAvg`, `co2` or `temp`. They
  are not an enum, so sensors added to the API later still come through.
- `Unit::Metric` (the default) and `Unit::Imperial` choose the units Airthings converts values to.
- Airthings can return `null` sensor entries. They are dropped, so `sensors` only holds real readings.
- `recorded` is the timestamp Airthings reports, as a string. `battery_percentage` is `None` for devices
  without a battery.
- `RateLimit` comes from the `X-RateLimit-*` headers of the last sensors page. Fields the server leaves out are
  `None`.
- All response types implement `Serialize` and `Deserialize`, so you can cache or forward them as JSON.

## Making API calls through a session

`Session::call` takes a closure that receives a valid access token and returns a future:

```rust,ignore
session.call(|token| async move { session.api().devices(&token, account).await }).await?
```

Before calling the closure, the session returns its cached token, or gets a new one if the cached token
expires within a minute. If the call fails with `Error::Unauthorized` (HTTP 401), the session drops the token,
gets a new one and calls the closure once more.

The closure may run twice, so it is `Fn`, and the `async move` block may only capture things that are `Copy`.
That is why the examples rebind the session and account ID as references before calling:

```rust,ignore
let account = session.account_id().await?;
let (session, account) = (&session, account.as_str());
```

To send several requests with one token, call `session.access_token()` yourself, at the cost of the automatic
retry. Concurrent callers share one token refresh: while one task is refreshing, the others wait for its result.

A `Session` is `Send + Sync`. Put it in an `Arc` to share it between tasks:

```rust,no_run
use std::sync::Arc;

use airthings_client::api::Unit;
use airthings_client::session::Session;

fn poll_in_background(session: Arc<Session>) {
    tokio::spawn(async move {
        let session = &*session;
        let account = session.account_id().await?;
        let account = account.as_str();
        let (readings, _) =
            session.call(|t| async move { session.api().sensors(&t, account, Unit::Metric).await }).await?;
        println!("{} devices", readings.len());
        Ok::<_, airthings_client::error::Error>(())
    });
}
```

## Storing credentials between runs

`MemoryStore` forgets everything when the process exits. To keep users signed in, implement `SecretStore` on
top of the OS keychain (the [`keyring`](https://crates.io/crates/keyring) crate), an encrypted file or your own
storage. `Credentials` is `Serialize` and `Deserialize`, so storing it as JSON is enough.

```rust,no_run
use std::path::PathBuf;

use airthings_client::auth::Credentials;
use airthings_client::error::{Error, Result};
use airthings_client::store::SecretStore;

/// Example only: a real app should keep secrets somewhere safer than a plain file.
struct FileStore(PathBuf);

impl SecretStore for FileStore {
    fn load(&self) -> Option<Credentials> {
        let json = std::fs::read_to_string(&self.0).ok()?;
        serde_json::from_str(&json).ok()
    }

    fn save(&self, credentials: &Credentials) -> Result<()> {
        let json = serde_json::to_string(credentials).map_err(|e| Error::Other(e.to_string()))?;
        std::fs::write(&self.0, json).map_err(|e| Error::Other(e.to_string()))
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
```

The session calls `save` after a sign-in and whenever Airthings rotates the refresh token, and `clear` on
sign-out. If `save` fails, the sign-in or token refresh returns that error. `SecretStore` must be
`Send + Sync`, and its methods are synchronous, so keep them quick.

To restore a sign-in, create the session with the store and check `is_signed_in()`:

```rust,no_run
use std::sync::Arc;

use airthings_client::api::{ApiClient, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, DEFAULT_TOKEN_URL};
use airthings_client::session::Session;
use airthings_client::store::SecretStore;

fn open_session(store: Arc<dyn SecretStore>) -> Session {
    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        store,
    );
    if !session.is_signed_in() {
        println!("no saved sign-in, ask the user to sign in");
    }
    session
}

```

## Handling errors

Every fallible call returns `airthings_client::error::Result<T>`. The `Error` variants are:

| Variant                    | When                                                                                     |
|----------------------------|------------------------------------------------------------------------------------------|
| `Http(reqwest::Error)`     | The request failed to send or the response could not be read or parsed                   |
| `Api { status, message }`  | Airthings answered with another error status. `message` is the error text it sent        |
| `RateLimited(seconds)`     | HTTP 429. `seconds` comes from `X-RateLimit-Retry-After` or `Retry-After`, otherwise 60  |
| `Unauthorized`             | Not signed in, or a token was still refused after a refresh                              |
| `Rejected(message)`        | The token endpoint refused the credentials (HTTP 400 or 401)                             |
| `Other(message)`           | Anything else, such as a sign-in timeout, a `state` mismatch or a busy redirect port     |

`Error` implements `std::error::Error` and `Display`, so it works with `?`, `anyhow` and `Box<dyn Error>`.

A polling loop that backs off when rate limited and stops when the user has to sign in again:

```rust,no_run
use std::time::Duration;

use airthings_client::api::Unit;
use airthings_client::error::Error;
use airthings_client::session::Session;

async fn poll(session: &Session) {
    loop {
        let result = async {
            let account = session.account_id().await?;
            let account = account.as_str();
            session.call(|t| async move { session.api().sensors(&t, account, Unit::Metric).await }).await
        }
        .await;

        let wait = match result {
            Ok((readings, _)) => {
                println!("{} devices", readings.len());
                Duration::from_secs(300)
            }
            Err(Error::RateLimited(seconds)) => Duration::from_secs(seconds),
            Err(Error::Unauthorized) => {
                eprintln!("signed out, sign in again");
                return;
            }
            Err(e) => {
                eprintln!("poll failed: {e}");
                Duration::from_secs(60)
            }
        };
        tokio::time::sleep(wait).await;
    }
}
```

## Using the clients without a session

`AuthClient` and `ApiClient` work on their own when you manage tokens yourself:

```rust,no_run
use airthings_client::api::{ApiClient, Unit, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, DEFAULT_SCOPE, DEFAULT_TOKEN_URL};

#[tokio::main]
async fn main() -> airthings_client::error::Result<()> {
    let http = reqwest::Client::new();
    let auth = AuthClient::new(http.clone(), DEFAULT_TOKEN_URL);
    let api = ApiClient::new(http, DEFAULT_BASE_URL);

    let token = auth.client_credentials("your-client-id", "your-client-secret", DEFAULT_SCOPE).await?;
    println!("token valid for {:?} seconds", token.expires_in);

    let accounts = api.accounts(&token.access_token).await?;
    let (readings, _) = api.sensors(&token.access_token, &accounts[0].id, Unit::Metric).await?;
    println!("{} devices", readings.len());
    Ok(())
}
```

`AuthClient` also has `exchange_code`, `refresh` and `token_for(&Credentials)`. The `auth` module exposes the
pieces of the browser flow (`Pkce::generate`, `random_token`, `redirect_uri`, `authorize_url`, `parse_callback`
and `wait_for_code`) if you want to run it your own way, for example with your own web server as the redirect
target. Hand the resulting token to `Session::complete_sign_in` to continue with a session.

## Testing against a mock server

Both clients take their base URL, so you can point them at a local mock server such as
[`wiremock`](https://crates.io/crates/wiremock):

```rust,no_run
use std::sync::Arc;

use airthings_client::api::ApiClient;
use airthings_client::auth::{AuthClient, Credentials};
use airthings_client::session::Session;
use airthings_client::store::MemoryStore;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn reads_the_account() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "t", "expires_in": 3600})))
        .mount(&server)
        .await;
    Mock::given(path("/v1/accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accounts": [{"id": "acc-1"}]})))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), format!("{}/v1/token", server.uri())),
        ApiClient::new(http, server.uri()),
        Arc::new(MemoryStore::default()),
    );
    let credentials =
        Credentials::ClientCredentials { client_id: "id".into(), client_secret: "s".into(), scope: "read".into() };
    session.sign_in(credentials).await.unwrap();

    assert_eq!(session.account_id().await.unwrap(), "acc-1");
}
```

## Defaults

| Constant                       | Value                                         |
|--------------------------------|-----------------------------------------------|
| `api::DEFAULT_BASE_URL`        | `https://consumer-api.airthings.com`          |
| `auth::DEFAULT_TOKEN_URL`      | `https://accounts-api.airthings.com/v1/token` |
| `auth::DEFAULT_AUTHORIZE_URL`  | `https://accounts.airthings.com/authorize`    |
| `auth::DEFAULT_SCOPE`          | `read:device:current_values`                  |
| `auth::DEFAULT_REDIRECT_PORT`  | `43682`                                       |
| `auth::CALLBACK_PATH`          | `/callback`                                   |

Access tokens are renewed 60 seconds before they expire. A token without an `expires_in` is assumed to last an
hour. The sensors endpoint is followed for at most 50 pages per call.

## License

MIT
