# airdash-core

The parts of [AirDash](https://github.com/christianhelle/airdash) that are neither UI nor the Airthings client:
fetching everything the dashboard shows in one refresh, keeping sign-in credentials in the OS keychain, and
saving user settings and the last known dashboard state as JSON.

It builds on [`airthings-client`](https://crates.io/crates/airthings-client), which does the sign-in, token
handling and API calls. Use this crate if you are building your own Airthings dashboard, tray app or CLI and
want the same storage and refresh behavior as AirDash.

| Module     | What it holds                                                                               |
|------------|---------------------------------------------------------------------------------------------|
| `refresh`  | `fetch`: one call that returns every device and its latest readings as a `Snapshot`          |
| `secrets`  | `KeychainStore`: a `SecretStore` backed by the OS keychain, with a file fallback             |
| `settings` | `Settings`: units, theme, the user's OAuth client, redirect port and scope                   |
| `state`    | `AppState`: the last `Snapshot`, when it was taken and the user's device order               |

This crate is not affiliated with or endorsed by Airthings.

## Installation

```sh
cargo add airdash-core airthings-client
cargo add tokio --features macros,rt-multi-thread
cargo add reqwest@0.13 --no-default-features --features rustls
```

`airthings-client` and `reqwest` are needed to build the `Session` that `refresh::fetch` takes. Use the same
`reqwest` major version as `airthings-client` (currently 0.13).

## Putting it together

This is roughly how AirDash starts up: load the settings, open a session whose credentials live in the
keychain, show the saved state at once, then refresh and save the new state.

```rust,no_run
use std::path::PathBuf;
use std::sync::Arc;

use airdash_core::refresh;
use airdash_core::secrets::KeychainStore;
use airdash_core::settings::Settings;
use airdash_core::state::AppState;
use airthings_client::api::{ApiClient, DEFAULT_BASE_URL};
use airthings_client::auth::{AuthClient, Credentials, DEFAULT_TOKEN_URL};
use airthings_client::session::Session;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pick folders that suit your app, for example with the `dirs` crate.
    let config_dir = PathBuf::from("config");
    let data_dir = PathBuf::from("data");
    std::fs::create_dir_all(&data_dir)?;

    let settings = Settings::load(&config_dir.join("settings.json"));

    let http = reqwest::Client::new();
    let session = Session::new(
        AuthClient::new(http.clone(), DEFAULT_TOKEN_URL),
        ApiClient::new(http, DEFAULT_BASE_URL),
        Arc::new(KeychainStore::new(data_dir.join("credentials.json"))),
    );
    if !session.is_signed_in() {
        session
            .sign_in(Credentials::ClientCredentials {
                client_id: std::env::var("AIRTHINGS_CLIENT_ID")?,
                client_secret: std::env::var("AIRTHINGS_CLIENT_SECRET")?,
                scope: settings.scope.clone(),
            })
            .await?;
    }

    // Show what we had last time while the refresh runs.
    let state_path = data_dir.join("state.json");
    let mut state = AppState::load(&state_path);
    println!("{} devices from the last run", state.devices().len());

    state.snapshot = refresh::fetch(&session).await?;
    state.last_refresh = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64);
    state.save(&state_path)?;

    for device in state.devices() {
        println!("{} ({})", device.name, device.serial_number);
    }
    Ok(())
}
```

## Refreshing

`refresh::fetch(&session)` looks up the account, then gets the device list and the latest readings from every
page of the sensors endpoint. It returns them together as a `Snapshot`:

```rust,no_run
use airdash_core::refresh::{self, Snapshot};
use airthings_client::session::Session;

async fn show(session: &Session) -> airthings_client::error::Result<()> {
    let Snapshot { devices, readings, rate_limit } = refresh::fetch(session).await?;
    for device in &devices {
        let latest = readings.iter().find(|r| r.serial_number == device.serial_number);
        let values = latest.map(|r| r.sensors.len()).unwrap_or(0);
        println!("{}: {values} sensor values", device.name);
    }
    if let Some(limit) = rate_limit {
        println!("{:?} requests left", limit.remaining);
    }
    Ok(())
}
```

- Readings are always fetched in metric units. `Settings::unit` is for display, so convert when you show a
  value. Stored snapshots then stay comparable when the user switches units.
- `fetch` does nothing on a timer. Call it as often as you need, but stay within the Airthings rate limit.
  `snapshot.rate_limit` tells you what is left, and a `429` comes back as
  `airthings_client::error::Error::RateLimited(seconds)`.
- Errors are the `airthings-client` ones. `Error::Unauthorized` means the user has to sign in again.

## Storing credentials in the keychain

`KeychainStore` implements `airthings_client::store::SecretStore`. Hand it to `Session::new` and the session
loads saved credentials when it is created, saves them after a sign-in and when Airthings rotates the refresh
token, and clears them on sign-out.

```rust,no_run
use std::path::PathBuf;

use airdash_core::secrets::KeychainStore;

let store = KeychainStore::new(PathBuf::from("data/credentials.json"));
```

- Credentials go into the OS keychain: Windows Credential Manager, the macOS Keychain or the Secret Service
  on Linux. They are saved under the service `AirDash` and the account `airthings-credentials`, as the JSON
  form of `airthings_client::auth::Credentials`.
- When no keychain is available, or the secret is too large for it (Windows allows 2560 bytes per entry), the
  credentials are written to the fallback path instead. The folder is created if needed and, on Unix, the
  file is made readable by its owner only (`0600`). The file is not encrypted.
- After a successful keychain save, any fallback file is deleted. `load` reads the keychain first, then the
  file. `clear` removes both.

All `KeychainStore` instances share the same keychain entry, whatever their fallback path. Two apps using this
crate on the same machine therefore share one sign-in. If your app needs its own, implement `SecretStore`
yourself; the `airthings-client` README shows how.

## Settings

`Settings` holds the user's preferences and is stored as pretty-printed JSON:

| Field                  | Default                       | Meaning                                                          |
|------------------------|-------------------------------|------------------------------------------------------------------|
| `unit`                 | `Unit::Metric`                | Units to display readings in                                     |
| `theme`                | `"system"`                    | `"system"`, `"light"` or `"dark"`                                 |
| `oauth_client_id`      | `None`                        | The user's own Airthings OAuth client, for browser sign-in       |
| `oauth_client_secret`  | `None`                        | Its secret, if it is a confidential client                       |
| `redirect_port`        | `43682`                       | Loopback port for the OAuth redirect                             |
| `scope`                | `read:device:current_values`  | Scope requested with client credentials                          |

```rust,no_run
use std::path::Path;

use airdash_core::settings::Settings;
use airthings_client::api::Unit;

fn switch_to_imperial(path: &Path) -> airthings_client::error::Result<()> {
    let mut settings = Settings::load(path);
    settings.unit = Unit::Imperial;
    settings.save(path)
}
```

- `Settings::load` never fails. A missing or unreadable file gives the defaults, and a file with only some
  fields gets defaults for the rest, so adding fields later does not break old files.
- `load` also normalizes what it read: a port of `0` or a blank scope go back to the defaults, and a blank
  OAuth client ID or secret becomes `None`. Call `normalized()` yourself on settings the user edited in your
  UI before saving them.
- `save` creates the folder if needed.
- No OAuth client is built in. `oauth_client()` returns `Some((client_id, secret))` only when the user has set
  one up, ready for `Session::sign_in_oauth`:

```rust,no_run
use airdash_core::settings::Settings;
use airthings_client::session::Session;

async fn sign_in(session: &Session, settings: &Settings) -> airthings_client::error::Result<()> {
    let Some((client_id, client_secret)) = settings.oauth_client() else {
        println!("set up an OAuth client in the settings first");
        return Ok(());
    };
    session
        .sign_in_oauth(client_id, client_secret, settings.redirect_port, |url| {
            println!("Open this page to sign in:\n{url}");
            Ok(())
        })
        .await
}
```

## Saved dashboard state

`AppState` is what the dashboard showed last, so the next launch can show it before the first refresh
finishes:

- `snapshot`: the result of the last successful refresh
- `last_refresh`: when that was, in seconds since the Unix epoch
- `device_order`: serial numbers in the order the user arranged their devices

```rust,no_run
use std::path::Path;

use airdash_core::state::AppState;

fn move_to_top(path: &Path, serial_number: &str) -> airthings_client::error::Result<()> {
    let mut state = AppState::load(path);
    state.device_order.retain(|s| s != serial_number);
    state.device_order.insert(0, serial_number.to_owned());
    state.save(path)
}
```

- `AppState::load` never fails. A missing or unreadable file gives an empty state.
- `save` writes to `<name>.json.tmp` and renames it over the file, so a crash mid-write never leaves a broken
  file. The folder must already exist.
- `devices()` returns the snapshot's devices in the user's order. Devices missing from `device_order`, such
  as newly added ones, come after the arranged ones, sorted by home and then name.

## License

MIT
