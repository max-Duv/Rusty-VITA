#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=${BIN:-./target/release/vita49-chaos}
if [[ ! -x "$BIN" ]]; then echo "Missing $BIN; run scripts/build_rhel8.sh first" >&2; exit 2; fi
sudo -v
exec "$BIN" --profile bx01 probe --capture-backend tshark --seconds "${1:-5}"
