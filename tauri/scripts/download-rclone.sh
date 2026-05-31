#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$(cd "$SCRIPT_DIR/../src-tauri/binaries" && pwd)"
RCLONE_VERSION="v1.74.1"

mkdir -p "$BIN_DIR"

download_mac() {
  local arch="$1"
  local suffix="$2"
  local zip_name expected_sha256

  case "$arch" in
    arm64)
      zip_name="rclone-${RCLONE_VERSION}-osx-arm64.zip"
      expected_sha256="98c04f5f678fe87d435d6f4b1fe204103c5906b151357e631ba0111410691213"
      ;;
    x86_64)
      zip_name="rclone-${RCLONE_VERSION}-osx-amd64.zip"
      expected_sha256="4f10d7845422d8568e187a0f6813f124bca9b657ac7becd8bdf8508fa968a336"
      ;;
    *)
      echo "Unsupported macOS architecture: $arch" >&2
      return 1
      ;;
  esac

  local dest="$BIN_DIR/rclone-${suffix}"
  if [[ -x "$dest" ]] && "$dest" version 2>/dev/null | head -n 1 | grep -qx "rclone $RCLONE_VERSION"; then
    echo "rclone already present at $dest"
    return 0
  fi

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' RETURN

  local zip_path="$tmp_dir/rclone.zip"
  curl -fsSL "https://github.com/rclone/rclone/releases/download/${RCLONE_VERSION}/${zip_name}" -o "$zip_path"
  echo "$expected_sha256  $zip_path" | shasum -a 256 -c -
  ditto -x -k "$zip_path" "$tmp_dir/unzipped"

  local found
  found="$(find "$tmp_dir/unzipped" -type f -name rclone -perm +111 | head -n 1)"
  if [[ -z "$found" ]]; then
    echo "Downloaded archive did not contain rclone" >&2
    return 1
  fi

  cp "$found" "$dest"
  chmod +x "$dest"
  echo "Downloaded rclone to $dest"
}

download_windows() {
  local dest="$BIN_DIR/rclone-x86_64-pc-windows-msvc.exe"
  if [[ -x "$dest" ]] && "$dest" version 2>/dev/null | head -n 1 | grep -qx "rclone $RCLONE_VERSION"; then
    echo "rclone already present at $dest"
    return 0
  fi

  local tmp_dir zip_path found
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' RETURN

  zip_path="$tmp_dir/rclone.zip"
  curl -fsSL "https://github.com/rclone/rclone/releases/download/${RCLONE_VERSION}/rclone-${RCLONE_VERSION}-windows-amd64.zip" -o "$zip_path"
  unzip -q "$zip_path" -d "$tmp_dir/unzipped"
  found="$(find "$tmp_dir/unzipped" -type f -name rclone.exe | head -n 1)"
  if [[ -z "$found" ]]; then
    echo "Downloaded archive did not contain rclone.exe" >&2
    return 1
  fi

  cp "$found" "$dest"
  chmod +x "$dest"
  echo "Downloaded rclone to $dest"
}

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) download_mac "arm64" "aarch64-apple-darwin" ;;
      x86_64) download_mac "x86_64" "x86_64-apple-darwin" ;;
      *) echo "Unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*)
    download_windows
    ;;
  *)
    download_mac "x86_64" "x86_64-apple-darwin" || true
    download_mac "arm64" "aarch64-apple-darwin" || true
    download_windows || true
    ;;
esac
