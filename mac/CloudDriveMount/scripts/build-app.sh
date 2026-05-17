#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$PROJECT_DIR/.build/app/Cloud Drive Mount.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

bash "$SCRIPT_DIR/download-rclone.sh"

swift build --package-path "$PROJECT_DIR" -c release

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$PROJECT_DIR/.build/release/CloudDriveMount" "$MACOS_DIR/CloudDriveMount"
cp "$PROJECT_DIR/Info.plist" "$CONTENTS_DIR/Info.plist"
if [[ -n "${APP_VERSION:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APP_VERSION" "$CONTENTS_DIR/Info.plist"
fi
if [[ -n "${APP_BUILD:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APP_BUILD" "$CONTENTS_DIR/Info.plist"
fi
printf 'APPL????' > "$CONTENTS_DIR/PkgInfo"

if [[ -x "$PROJECT_DIR/vendor/rclone/rclone" ]]; then
  cp "$PROJECT_DIR/vendor/rclone/rclone" "$RESOURCES_DIR/rclone"
  chmod +x "$RESOURCES_DIR/rclone"
fi

codesign --force --deep --sign - "$APP_DIR" >/dev/null

echo "Built $APP_DIR"
