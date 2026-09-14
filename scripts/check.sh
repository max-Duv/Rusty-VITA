#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found. Install a current stable Rust toolchain first." >&2
  exit 2
fi

# Formatting is reported but does not hide compiler/test results. This is useful
# on lab hosts where a copied source tree may not have been rustfmt-normalized
# yet; cargo check/test remain the authoritative gates.
if cargo fmt -- --check; then
  echo "==> rustfmt: clean"
else
  echo "==> rustfmt: differences detected (run 'cargo fmt' to normalize)" >&2
fi

printf '\n==> cargo check\n'
cargo check
printf '\n==> cargo test\n'
cargo test

if command -v cargo-clippy >/dev/null 2>&1 || rustup component list 2>/dev/null | grep -q '^clippy.*(installed)'; then
  printf '\n==> cargo clippy (advisory)\n'
  cargo clippy || echo "clippy reported advisory findings" >&2
fi
