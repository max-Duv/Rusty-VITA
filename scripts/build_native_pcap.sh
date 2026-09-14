#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found. Install stable Rust first."
  exit 2
fi
cat <<'MSG'
Native capture requires libpcap development headers. On RHEL 8 this is commonly:
  sudo dnf install libpcap-devel
The proven Bx01 path does NOT require this feature; it uses tshark.
MSG
cargo build --release --features native-pcap
