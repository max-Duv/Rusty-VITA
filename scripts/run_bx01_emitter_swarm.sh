#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=${BIN:-./target/release/vita49-chaos}
if [[ ! -x "$BIN" ]]; then echo "Missing $BIN; run scripts/build_rhel8.sh first" >&2; exit 2; fi
sudo -n true >/dev/null 2>&1 || { echo "Run 'sudo -v' first so tshark can start non-interactively." >&2; exit 2; }
exec "$BIN" --profile bx01 --preset 'DEMO: Emitter swarm' live --capture-backend tshark
