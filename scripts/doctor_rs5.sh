#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/.."

PASS=0
WARN=0
FAIL=0
ok()   { printf '[ OK ] %s\n' "$*"; PASS=$((PASS+1)); }
warn() { printf '[WARN] %s\n' "$*"; WARN=$((WARN+1)); }
bad()  { printf '[FAIL] %s\n' "$*"; FAIL=$((FAIL+1)); }

printf 'VITA-49 Chaos Workbench — RS5 doctor\n\n'

if command -v rustc >/dev/null 2>&1; then ok "rustc: $(rustc --version)"; else bad 'rustc not found'; fi
if command -v cargo >/dev/null 2>&1; then ok "cargo: $(cargo --version)"; else bad 'cargo not found'; fi

if command -v tshark >/dev/null 2>&1; then
  TV=$(tshark -v 2>/dev/null | head -1)
  ok "$TV"
  if tshark -G fields 2>/dev/null | awk -F '\t' '{for(i=1;i<=NF;i++) if($i=="udp.payload") found=1} END{exit !found}'; then
    ok 'TShark raw payload field: udp.payload'
  elif tshark -G fields 2>/dev/null | awk -F '\t' '{for(i=1;i<=NF;i++) if($i=="data.data") found=1} END{exit !found}'; then
    ok 'TShark raw payload field: data.data (expected for RS5 / Wireshark 2.6.2)'
  else
    bad 'Neither udp.payload nor data.data is exposed by tshark'
  fi
else
  bad 'tshark not found'
fi

if ip link show bridge0 >/dev/null 2>&1; then ok 'bridge0 exists'; else bad 'bridge0 not found'; fi
if ip link show enp4s0f0 >/dev/null 2>&1; then ok 'enp4s0f0 exists'; else warn 'physical ingress enp4s0f0 not found'; fi

if [[ -n "${DISPLAY:-}" ]]; then ok "DISPLAY=$DISPLAY"; else warn 'DISPLAY is unset; GUI launch will fail in a non-graphical shell'; fi

if sudo -n true >/dev/null 2>&1; then
  ok 'sudo credential cache is ready for sudo -n tshark'
else
  warn 'sudo -n is not ready; run: sudo -v'
fi

BIN=./target/release/vita49-chaos
if [[ -x "$BIN" ]]; then ok "release binary exists: $BIN"; else warn 'release binary not built yet; run ./scripts/build_rhel8.sh'; fi

printf '\nPassive Bx01 wire check (2 seconds) ...\n'
if command -v timeout >/dev/null 2>&1 && command -v tcpdump >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then
  COUNT=$(sudo -n timeout 2 tcpdump -ni bridge0 -c 20 'src host 174.168.1.189 and dst host 239.254.253.252' 2>/dev/null | wc -l || true)
  if [[ "$COUNT" -gt 0 ]]; then ok "Bx01 multicast traffic is visible on bridge0"; else warn 'no Bx01 packets observed during the 2-second wire check'; fi
else
  warn 'wire check skipped (needs tcpdump, timeout, and refreshed sudo credentials)'
fi

printf '\nSummary: %d OK, %d warning(s), %d failure(s)\n' "$PASS" "$WARN" "$FAIL"
if [[ "$FAIL" -gt 0 ]]; then exit 2; fi
