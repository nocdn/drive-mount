#!/bin/bash
# Notarizes a signed DMG with Apple and staples the ticket to it, so it opens
# without a Gatekeeper warning even offline. cargo-packager already notarizes
# the app inside; this covers the disk image itself.
#
# Usage: scripts/notarize-dmg.sh [path/to/app.dmg]
# Without a path it takes the newest DMG in desktop/target/release/bundle.
#
# Credentials, the same ones cargo-packager reads:
#   APPLE_KEYCHAIN_PROFILE  a `xcrun notarytool store-credentials` profile, or
#   APPLE_API_KEY, APPLE_API_ISSUER, APPLE_API_KEY_PATH  an App Store Connect API key.
set -euo pipefail

dmg="${1:-}"
if [[ -z "$dmg" ]]; then
  bundle_dir="$(cd "$(dirname "$0")/.." && pwd)/desktop/target/release/bundle"
  dmg="$(find "$bundle_dir" -maxdepth 1 -name '*.dmg' ! -name 'rw.*' -print0 |
    xargs -0 ls -t 2>/dev/null | head -n 1)"
fi
if [[ -z "$dmg" || ! -f "$dmg" ]]; then
  echo "No DMG to notarize${dmg:+ at $dmg}." >&2
  exit 1
fi

if [[ -n "${APPLE_KEYCHAIN_PROFILE:-}" ]]; then
  auth=(--keychain-profile "$APPLE_KEYCHAIN_PROFILE")
elif [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
  auth=(--key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --key "$APPLE_API_KEY_PATH")
else
  echo "No notarization credentials: set APPLE_KEYCHAIN_PROFILE, or APPLE_API_KEY, APPLE_API_ISSUER and APPLE_API_KEY_PATH." >&2
  exit 1
fi

echo "Notarizing $dmg"
result="$(xcrun notarytool submit "$dmg" "${auth[@]}" --wait --output-format plist)"
status="$(/usr/libexec/PlistBuddy -c 'Print :status' /dev/stdin <<<"$result")"
if [[ "$status" != "Accepted" ]]; then
  id="$(/usr/libexec/PlistBuddy -c 'Print :id' /dev/stdin <<<"$result" || true)"
  echo "Notarization finished with status: $status" >&2
  [[ -n "$id" ]] && xcrun notarytool log "$id" "${auth[@]}" >&2 || true
  exit 1
fi

xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
spctl --assess --type open --context context:primary-signature --verbose "$dmg"
