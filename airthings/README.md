# airthings

A Rust client for the [Airthings Consumer API](https://consumer-api-doc.airthings.com/): sign in, keep a valid
access token and read your devices and their latest sensor values.

- Client-credentials sign-in, or OAuth authorization code with PKCE through the user's browser and a loopback
  redirect
- Access tokens are cached and refreshed before they expire, rotated refresh tokens are kept, and a call that
  gets a 401 is retried once with a new token
- Pluggable credential storage through the `SecretStore` trait, with an in-memory store included
- Rate-limit headers and `429` responses are surfaced as typed values

Create an API client under [Airthings My Applications](https://consumer-api-doc.airthings.com/dashboard) to get
a client ID and secret.

```rust,no_run
use std::sync::Arc;

use airthings::api::{ApiClient, Unit, DEFAULT_BASE_URL};
use airthings::auth::{AuthClient, Credentials, DEFAULT_SCOPE, DEFAULT_TOKEN_URL};
use airthings::session::Session;
use airthings::store::MemoryStore;

#[tokio::main]
async fn main() -> airthings::error::Result<()> {
    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        Arc::new(MemoryStore::default()),
    );
    session
        .sign_in(Credentials::ClientCredentials {
            client_id: "your-client-id".into(),
            client_secret: "your-client-secret".into(),
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

This crate is not affiliated with or endorsed by Airthings.

## License

MIT
