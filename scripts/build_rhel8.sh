#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found. Install a current stable Rust toolchain first."
  echo "Preferred: your organization's approved Rust/rustup installation method."
  exit 2
fi
printf '==> cargo test\n'
cargo test
printf '\n==> cargo build --release\n'
cargo build --release
printf '\nBuilt: %s\n' "$(pwd)/target/release/vita49-chaos"
