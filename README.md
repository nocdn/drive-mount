# Cloud Drive Mount

Cross-platform Tauri app for mounting cloud storage as a local drive on macOS and Windows, powered by `rclone`.

## Development

- `bun install`
- `bun run tauri dev`

## Installer Builds

The Tauri bundler is enabled in `src-tauri/tauri.conf.json`. Platform-specific Tauri config files select the installer type:

- macOS: `src-tauri/tauri.macos.conf.json` builds a DMG.
- Windows: `src-tauri/tauri.windows.conf.json` builds an MSI only.

Build commands:

- `bun run build:installer` builds the installer for the current platform using Tauri's platform config merge.
- `bun run build:installer:mac` builds a macOS DMG on macOS.
- `bun run build:installer:windows` builds a Windows MSI on Windows.

Installer outputs are written under `src-tauri/target/release/bundle/`. The Tauri build runs `bun run prepare:sidecars` first so the platform-specific `rclone` sidecar is bundled with the app.
