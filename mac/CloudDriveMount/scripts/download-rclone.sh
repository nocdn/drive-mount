#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENDOR_DIR="$PROJECT_DIR/vendor/rclone"
RCLONE_PATH="$VENDOR_DIR/rclone"
RCLONE_VERSION="v1.74.1"

if [[ -x "$RCLONE_PATH" ]] && "$RCLONE_PATH" version | head -n 1 | grep -qx "rclone $RCLONE_VERSION"; then
  exit 0
fi

mkdir -p "$VENDOR_DIR"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)
    ZIP_NAME="rclone-$RCLONE_VERSION-osx-arm64.zip"
    EXPECTED_SHA256="98c04f5f678fe87d435d6f4b1fe204103c5906b151357e631ba0111410691213"
    ;;
  x86_64)
    ZIP_NAME="rclone-$RCLONE_VERSION-osx-amd64.zip"
    EXPECTED_SHA256="4f10d7845422d8568e187a0f6813f124bca9b657ac7becd8bdf8508fa968a336"
    ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

ZIP_PATH="$TMP_DIR/rclone.zip"
DOWNLOAD_URL="https://github.com/rclone/rclone/releases/download/$RCLONE_VERSION/$ZIP_NAME"
curl -fsSL "$DOWNLOAD_URL" -o "$ZIP_PATH"
echo "$EXPECTED_SHA256  $ZIP_PATH" | shasum -a 256 -c -
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
