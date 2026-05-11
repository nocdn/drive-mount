#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENDOR_DIR="$PROJECT_DIR/vendor/rclone"
RCLONE_PATH="$VENDOR_DIR/rclone"

if [[ -x "$RCLONE_PATH" ]]; then
  exit 0
fi

mkdir -p "$VENDOR_DIR"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)
    ASSET_PATTERN='osx-arm64.zip'
    ;;
  x86_64)
    ASSET_PATTERN='osx-amd64.zip'
    ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

RELEASE_JSON="$TMP_DIR/release.json"
curl -fsSL -H 'User-Agent: CloudDriveMount' 'https://api.github.com/repos/rclone/rclone/releases/latest' -o "$RELEASE_JSON"

DOWNLOAD_URL="$(/usr/bin/python3 - "$RELEASE_JSON" "$ASSET_PATTERN" <<'PY'
import json
import sys

release_path, pattern = sys.argv[1], sys.argv[2]
with open(release_path, 'r', encoding='utf-8') as f:
    release = json.load(f)

for asset in release.get('assets', []):
    if pattern in asset.get('name', ''):
        print(asset.get('browser_download_url', ''))
        break
PY
)"

if [[ -z "$DOWNLOAD_URL" ]]; then
  echo "Could not find rclone release asset matching $ASSET_PATTERN" >&2
  exit 1
fi

ZIP_PATH="$TMP_DIR/rclone.zip"
curl -fsSL "$DOWNLOAD_URL" -o "$ZIP_PATH"
ditto -x -k "$ZIP_PATH" "$TMP_DIR/unzipped"

FOUND="$(/usr/bin/python3 - "$TMP_DIR/unzipped" <<'PY'
import os
import sys

root = sys.argv[1]
for directory, _, files in os.walk(root):
    if 'rclone' in files:
        path = os.path.join(directory, 'rclone')
        if os.access(path, os.X_OK):
            print(path)
            break
PY
)"
if [[ -z "$FOUND" ]]; then
  echo "Downloaded archive did not contain rclone" >&2
  exit 1
fi

cp "$FOUND" "$RCLONE_PATH"
chmod +x "$RCLONE_PATH"
echo "Downloaded rclone to $RCLONE_PATH"
