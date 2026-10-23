# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- Install the latest release on Linux and macOS with `curl -fsSL https://christianhelle.com/airdash/install | bash`, which also adds AirDash to the Linux app launcher, and remove it again with `--uninstall`.
- Install the latest release on Windows with a PowerShell script, which adds AirDash to the Start Menu and, with `-AddToPath`, `airdash.exe` to the `PATH`.
- Install on Arch Linux and Omarchy from the AUR, with `airdash-bin` for the prebuilt release or `airdash` to build from source.

### Fixed

- Hyprland no longer reports AirDash as not responding after it has sat on another workspace for a while.
- AirDash no longer takes 2 seconds longer to start on Windows when Hyper-V, WSL or Docker has reserved its port.

### Changed

- The default OAuth redirect URI is now `http://127.0.0.1:43682/callback`, as Windows often reserves the old port 53682. A redirect port already saved in the settings is kept.

## [0.1.0] - 2026-10-17

First release of AirDash, a desktop dashboard for Airthings air quality monitors.

### Added

- Dashboard of every device on the account, grouped by home, with the latest sensor values rated Good, Fair or Poor using Airthings' thresholds.
- Reorder devices by dragging their cards.
- Refresh every 5 minutes while open, or right away with Refresh in the status bar, which also shows the API rate-limit headroom.
- Show the last known readings, device order and window size and position at once on launch.
- Sign in with an API client from Airthings My Applications, or through the browser with an OAuth client, with credentials kept in the OS keychain.
- Settings for metric or imperial units and a light, dark or system theme.
- Launching AirDash again brings the running window to the front instead of starting a second one.
- Screen reader labels on the sign-in screen, the settings and the device cards.
- Release builds for Linux, macOS and Windows on x64 and ARM64, with a desktop entry and icon in the Linux archives.
- Install with `cargo install airdash` from crates.io, along with the `airthings-client` and `airdash-core` crates.

[0.1.0]: https://github.com/christianhelle/airdash/releases/tag/v0.1.0
