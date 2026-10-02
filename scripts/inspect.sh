#!/usr/bin/env bash
# XOKSA release-artifact shipping inspection — the automatable checks of the
# shipping-inspection record in docs/dev-prog/security-assessment.md, in one command.
#
# Usage:  scripts/inspect.sh <path-to-xoksa-binary> [port]
# Example (Linux/macOS):  scripts/inspect.sh ./xoksa-linux 8799
# Example (Windows, Git Bash):  scripts/inspect.sh ./xoksa-windows.exe 8799
#
# Runs on any OS with bash + curl (Git Bash on Windows). Three checks stay MANUAL
# and are printed at the end: (a) SHA-256 vs the published <asset>.sha256,
# (b) the browser render, (c) the live Stooq failover test.
set -u

BIN="${1:?usage: scripts/inspect.sh <xoksa-binary> [port]}"
PORT="${2:-8799}"
B="http://127.0.0.1:${PORT}"
TMP="$(mktemp -d)"
pass=0; fail=0
ck(){ if [ "$3" = "$2" ]; then echo "PASS  $1"; pass=$((pass+1));
      else echo "FAIL  $1 (expected $2, got $3)"; fail=$((fail+1)); fi; }

echo "== inspecting: ${BIN} =="
"$BIN" --version || { echo "FAIL: binary will not run"; exit 1; }

"$BIN" serve --port "$PORT" --host 127.0.0.1 --private > "$TMP/serve.log" 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null; rm -rf "$TMP"' EXIT
for _ in $(seq 1 60); do
  [ "$(curl -s -o /dev/null -w '%{http_code}' "$B/api/health" 2>/dev/null)" = "200" ] && break
  sleep 0.5
done

ck "1 embedded UI  GET /            -> 200" 200 \
   "$(curl -s -o /dev/null -w '%{http_code}' "$B/")"
ck "2 CSP header present            -> yes" 1 \
   "$(curl -s -D - -o /dev/null "$B/" | grep -ci content-security-policy)"
ck "3 cross-origin POST             -> 403" 403 \
   "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/backtest/rules" \
      -H 'Content-Type: application/json' -H 'Origin: http://evil.example' \
      -d '{"name":"x","spec_json":"{}"}')"
{ printf '{"name":"b","spec_json":"'; head -c 70000 /dev/zero | tr '\0' A; printf '"}'; } > "$TMP/big.json"
ck "4 oversized body (>64 KiB)      -> 413" 413 \
   "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/backtest/rules" \
      -H 'Content-Type: application/json' -H "Origin: $B" --data-binary @"$TMP/big.json")"
ck "5 text/plain POST              -> 415" 415 \
   "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/backtest/rules" \
      -H 'Content-Type: text/plain' -H "Origin: $B" -d '{"name":"x","spec_json":"{}"}')"
ck "6 same-origin small POST        -> 200" 200 \
   "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/backtest/rules" \
      -H 'Content-Type: application/json' -H "Origin: $B" -d '{"name":"probe","spec_json":"{}"}')"

"$BIN" --ticker AAPL --no-llm --out "$TMP/a.txt" >/dev/null 2>&1
if [ -s "$TMP/a.txt" ]; then echo "PASS  7 CLI analysis produces a report"; pass=$((pass+1))
else echo "FAIL  7 CLI analysis"; fail=$((fail+1)); fi
if curl -s -X POST "$B/api/backtest" -H 'Content-Type: application/json' -H "Origin: $B" \
     -d '{"symbol":"AAPL"}' | grep -q total_return_pct; then
  echo "PASS  8 backtest API returns a result"; pass=$((pass+1))
else echo "FAIL  8 backtest API"; fail=$((fail+1)); fi

# 9 - the artifact must not name the account that built it. rustc records each
# crate's source path for panic messages, so a binary built on a developer
# machine carries that account name; a runner's does not identify anyone.
if bash "$(dirname "$0")/check-no-host-paths.sh" "$BIN" >/dev/null 2>&1; then
  echo "PASS  9 no identifying build path in the binary"; pass=$((pass+1))
else
  echo "FAIL  9 the binary names the account that built it (run the check for detail)"
  fail=$((fail+1))
fi

echo
echo "== automatable checks: ${pass} passed, ${fail} failed =="
echo "Still MANUAL (see security-assessment.md):"
echo "  a) SHA-256:  compare the binary's hash to the published <asset>.sha256"
echo "  b) browser:  <chrome|msedge> --headless=new --dump-dom ${B}/  -> app is mounted"
echo "  c) failover: cargo test -- --ignored failover_adopts_stooq  (on a Stooq-reachable network)"
[ "$fail" -eq 0 ]
