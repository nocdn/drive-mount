# Cloud Drive Mount

Cross-platform desktop app for mounting cloud storage as a local drive on macOS and Windows, powered by `rclone`.

The app is native Rust. Its window is drawn with [Rune](https://github.com/anomalyco/rune), Anomaly's GPU UI framework, using the `rune-kit` components and the OpenCode theme, laid out like [Hex](https://github.com/anomalyco/hex): a sidebar of panes, cards of setting rows, select menus and an overflow menu for less common actions. The source lives in `desktop/`.

## Requirements

- macOS: [FUSE-T](https://www.fuse-t.org/) (`brew install --cask fuse-t`). It mounts without a kernel extension, by serving each drive to macOS's own NFS client, so there's no system extension to approve and no Reduced Security mode. The first time an app opens a drive, macOS may ask to allow access to network volumes.
- Windows: [WinFsp](https://winfsp.dev/).

## Development

- `bun run dev` downloads the `rclone` sidecar for this machine and runs the app.
- macOS: `cargo run` (through the runner in `.cargo/config.toml`) signs the app with the "Developer ID Application: Bartosz Bak (7CAU3XFRLQ)" identity, as the installers are. The keychain trusts apps by team and bundle identifier, so dev builds, local installers and releases count as one app, and it asks for access once rather than after every rebuild or update. Without that identity, dev builds run unsigned.
- `bun run test` runs the Rust tests and the script tests.
- `cargo run --manifest-path desktop/Cargo.toml -- --list-scenes` lists the fixture screens, and `-- --snap out.png --scene drives` renders one to a PNG without touching your settings or mounts (add `--theme light` for the light theme).

## Installer Builds

Installers are built with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) (`cargo install cargo-packager --locked`), configured under `[package.metadata.packager]` in `desktop/Cargo.toml`.

- `bun run build:installer:mac` builds a macOS DMG on macOS.
- `bun run build:installer:mac:notarized` also notarizes the app and the DMG with Apple and staples the tickets. It uses the `cloud-drive-mount` notarytool profile; create it once with `xcrun notarytool store-credentials cloud-drive-mount --key AuthKey_KEYID.p8 --key-id KEYID --issuer ISSUER_ID`.
- `bun run build:installer:windows` builds a Windows MSI on Windows. Its WiX template, `desktop/packaging/windows/main.wxs`, keeps the upgrade code the Tauri build used, so it replaces an installed copy.
- `bun run build:installer` builds every default format for the current platform.
- `bun run build:installer:open` checks the installed app version, bumps the project patch version when needed, builds the current platform installer, finds the newest `.dmg` or `.msi` under `desktop/target/release/bundle/`, and opens it with `open` on macOS or `Start-Process` on Windows.

Installer outputs are written under `desktop/target/release/bundle/`. Each build script runs `bun run prepare:sidecars` first so the platform-specific `rclone` sidecar is bundled beside the app.

Releases (`.github/workflows/build-release.yml`, on every push to `main`) are signed and notarized on macOS. The workflow needs these repository secrets and fails without them:

- `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`: the Developer ID Application identity as a base64 `.p12` export, and its password.
- `APPLE_API_KEY` and `APPLE_API_ISSUER`: an App Store Connect API key's ID and issuer ID.
- `APPLE_API_PRIVATE_KEY_BASE64`: that key's `.p8` file, base64 encoded.
