# AirDash

A cross-platform desktop dashboard for Airthings air quality monitors. It replaces the retiring
[Airthings Web Dashboard](https://dashboard.airthings.com/).

Built with [Tauri 2](https://tauri.app) (Rust) and Svelte. There is no server: the app talks directly to the
[Airthings Consumer API](https://consumer-api-doc.airthings.com/) from your computer.

> AirDash is an independent project and is not affiliated with or endorsed by Airthings.

## Features

- Every device on your account, grouped by home, with live sensor values (radon, CO₂, VOC, PM, humidity,
  temperature, pressure and more). Each value is rated Good, Fair or Poor using Airthings' thresholds.
- History charts covering 24 hours, 7 days, 30 days, 1 year or everything, with CSV export.
- Mode and fan-speed control for Airthings Renew air purifiers.
- Metric or imperial units, plus light, dark or system theme.
- Keeps recording in the system tray while the window is closed.
- Credentials are stored in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service on Linux).

### How history works

The Consumer API only returns the **latest** reading for each device. AirDash therefore polls every 5 minutes
by default (Airthings allows 120 requests per hour) and saves each reading to a local SQLite database. Charts
start empty and fill in for as long as AirDash is running, so keep it in the tray to record continuously.

To find the data folder, open Settings → Local data → Open folder.

## Signing in

There are two ways to sign in:

1. **Sign in with Airthings.** Available in builds that include a registered OAuth client. AirDash opens the
   Airthings login page in your browser and receives the result on `http://127.0.0.1:53682/callback`. No API
   keys are needed.
2. **Use your own API client.** Works with every build. Create an API client under
   [Airthings My Applications](https://consumer-api-doc.airthings.com/dashboard) and paste its client ID and
   secret into AirDash once.

### Registering the AirDash OAuth client

Airthings only lets *business* accounts create OAuth clients that use the authorization code flow. Consumer
accounts can create client-credentials keys only. To enable browser sign-in for everyone:

1. With an Airthings business account, or with help from Airthings support, create an API client with:
   - Flow type: **Authorization code** (`CODE_FLOW`)
   - Access type: **Public** if Airthings accepts PKCE without a secret, otherwise **Confidential**
   - Redirect URI: `http://127.0.0.1:53682/callback`
   - Scope: read access to devices
2. Build AirDash with the client compiled in:

   ```bash
   AIRDASH_CLIENT_ID=<client id> AIRDASH_CLIENT_SECRET=<secret, if confidential> npm run tauri build
   ```

   In CI, add `AIRDASH_CLIENT_ID` and `AIRDASH_CLIENT_SECRET` as repository secrets. The release workflow
   passes them to the build.

A secret shipped inside a desktop app cannot be kept truly secret. A public client with PKCE is preferable
whenever Airthings allows one.

You can also enter a different OAuth client and redirect port without rebuilding, under Settings → Advanced
sign-in options.

## Development

Prerequisites: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 22+, and the
[Tauri system dependencies](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev
```

Tests and checks:

```bash
npm test
npm run check
cargo test --manifest-path core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Release build (installers end up in `src-tauri/target/release/bundle`):

```bash
npm run tauri build
```

### Pure Rust version (egui)

`egui-app/` is the same app without a web view: an [egui](https://github.com/emilk/egui) UI on top of the shared
`airdash-core` crate. It has the same features (dashboard, history charts, CSV export, Renew controls, settings,
tray icon) and uses the same settings, history database and stored sign-in as the Tauri app, so you can switch
between them. It needs only Rust to build:

```bash
cargo run --manifest-path egui-app/Cargo.toml
cargo test --manifest-path egui-app/Cargo.toml
cargo build --release --manifest-path egui-app/Cargo.toml
```

The OAuth client is compiled in the same way, through `AIRDASH_CLIENT_ID` and `AIRDASH_CLIENT_SECRET`. On Linux
the tray icon uses the StatusNotifierItem D-Bus protocol, so GTK is not needed.

### Layout

| Path | What |
| --- | --- |
| `core/src/api.rs` | Typed Consumer API client |
| `core/src/auth.rs` | OAuth (PKCE and loopback redirect) and client-credentials token requests |
| `core/src/session.rs` | Access-token caching, refresh, the retry on 401 and the browser sign-in flow |
| `core/src/secrets.rs` | Keychain credential storage |
| `core/src/store.rs` | SQLite reading history |
| `core/src/poller.rs` | Background polling that respects rate limits |
| `src-tauri/src/commands.rs` | Commands the Svelte UI calls |
| `src/lib/` | Svelte UI: API wrappers, sensor metadata, components and views |
| `egui-app/src/` | egui UI: backend wiring, views, charts, tray and sensor metadata |

## License

MIT
