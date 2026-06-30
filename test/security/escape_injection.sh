#!/usr/bin/env bash
#
# Security test: terminal escape-sequence injection.
#
# src/main.rs prints input paths to the terminal with no sanitization, so any
# control bytes in a path reach the terminal raw. The tool's headline use case
# is `find . | as-tree`, and on Unix a filename may contain any byte except `/`
# and NUL -- including ESC. A crafted filename can therefore:
#   B1  inject ANSI SGR codes to spoof/recolor output;
#   B2  inject an OSC sequence to set the window title, or on terminals that
#       honor OSC 52, write to the clipboard;
#   B3  inject a carriage return to redraw the line and hide the real path.
#
# All cases use `--color never`, so the tool emits NO escapes of its own; any
# control byte in the output came straight from the input path. Each test
# asserts the SECURE behavior (control bytes neutralized).
#
# RESOLVED by escaping control characters in path output before printing
# (src/main.rs: escape_control). This script is the acceptance test for that
# fix and lands together with it; it must stay green.
#
# Usage:
#   ./test/security/escape_injection.sh
#   AS_TREE_BIN=/path/to/as-tree ./test/security/escape_injection.sh
#
# Exit status: 0 if injection is neutralized, 1 if still exploitable.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

BIN="${AS_TREE_BIN:-}"
if [ -z "$BIN" ]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "Building as-tree (cargo build)..."
    ( cd "$REPO_ROOT" && cargo build --quiet ) || { echo "ERROR: cargo build failed" >&2; exit 2; }
    BIN="$REPO_ROOT/target/debug/as-tree"
  fi
fi
[ -n "$BIN" ] && [ -x "$BIN" ] || { echo "ERROR: as-tree binary not found; set AS_TREE_BIN." >&2; exit 2; }
echo "Testing binary: $BIN"
echo

_g() { printf '\033[32m%s\033[0m' "$1"; }
_r() { printf '\033[31m%s\033[0m' "$1"; }
nfail=0
pass() { printf '  [%s] %s\n' "$(_g PASS)" "$1"; }
fail() { nfail=$((nfail + 1)); printf '  [%s] %s\n' "$(_r FAIL)" "$1"; [ $# -ge 2 ] && printf '         -> %s\n' "$2"; }

# Render control bytes visibly (ESC -> ^[, BEL -> ^G, CR -> ^M) and search.
# `cat -v` is portable across GNU and BSD/macOS.
contains_marker() { cat -v "$1" | grep -q "$2"; }

echo "Terminal escape-sequence injection (src/main.rs)"

# B1: ANSI SGR (color) codes embedded in a path -> output spoofing.
printf 'safe/\033[31mLOOKS-RED\033[0m/leaf\n' | "$BIN" --color never >"$WORK/b1.out" 2>/dev/null
if contains_marker "$WORK/b1.out" '\^\['; then
  fail "B1 ANSI SGR escape in a path is neutralized" "raw ESC reached stdout -- output spoofing possible"
else
  pass "B1 ANSI SGR escape in a path is neutralized"
fi

# B2: OSC sequence (set window title / OSC 52 clipboard write) + BEL terminator.
printf 'safe/\033]0;PWNED-TITLE\007/leaf\n' | "$BIN" --color never >"$WORK/b2.out" 2>/dev/null
if contains_marker "$WORK/b2.out" '\^\[' || contains_marker "$WORK/b2.out" '\^G'; then
  fail "B2 OSC title/clipboard sequence is neutralized" "raw ESC/BEL reached stdout -- title spoof / clipboard write possible"
else
  pass "B2 OSC title/clipboard sequence is neutralized"
fi

# B3: carriage return mid-path -> redraw the line to hide the real path.
printf 'REAL-PATH\rFAKE-PATH/leaf\n' | "$BIN" --color never >"$WORK/b3.out" 2>/dev/null
if contains_marker "$WORK/b3.out" '\^M'; then
  fail "B3 carriage-return in a path is neutralized" "raw CR reached stdout -- line-overwrite spoofing possible"
else
  pass "B3 carriage-return in a path is neutralized"
fi

echo
if [ "$nfail" -ne 0 ]; then
  echo "$(_r VULNERABLE): $nfail/3 injection test(s) failing -- escape injection is NOT resolved."
  echo "(Expected until path output is escaped.)"
  exit 1
fi
echo "$(_g OK): escape injection resolved."
exit 0
