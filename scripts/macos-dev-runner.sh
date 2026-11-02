#!/bin/bash
# Cargo's runner for macOS (`cargo run`, `cargo test`): signs the app binary
# with the Developer ID identity the installers use, under the installed
# app's bundle identifier. The keychain trusts apps by team and identifier,
# so dev builds, local installers and CI releases are one app to it: a
# keychain "Always Allow" is asked for once, not after every rebuild. Test
# binaries, and machines without the identity, run unsigned as before.
set -euo pipefail

IDENTITY="Developer ID Application: Bartosz Bak (7CAU3XFRLQ)"

binary="$1"
if [[ "$(basename "$binary")" == "cloud-drive-mount" ]] &&
  security find-identity -v -p codesigning 2>/dev/null | grep -qF "\"$IDENTITY\""; then
  # No timestamp: it needs Apple's server, and only shipped builds need one.
  codesign --force --sign "$IDENTITY" --timestamp=none \
    --identifier com.bartek.clouddrivemount "$binary" 2>/dev/null ||
    echo "warning: could not sign $binary; the keychain may ask for access again" >&2
fi
exec "$@"
