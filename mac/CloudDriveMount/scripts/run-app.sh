#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

"$SCRIPT_DIR/build-app.sh"
osascript -e 'tell application id "com.bartek.clouddrivemount" to quit' >/dev/null 2>&1 || true
sleep 1
open "$PROJECT_DIR/.build/app/Cloud Drive Mount.app"
echo "Runtime log: $HOME/Library/Logs/CloudDriveMount/app.log"
