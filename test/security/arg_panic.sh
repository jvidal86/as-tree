#!/usr/bin/env bash
#
# Regression test: argument-parsing panic.
#
# src/options.rs checked `&arg[..1] == "-"`, slicing the argument by BYTE. An
# argument whose first character is multi-byte UTF-8 (e.g. `as-tree ñoño.txt`)
# put byte index 1 mid-character and panicked before the file was ever opened
# (exit 101, or SIGABRT under the release `panic = "abort"` profile) -- a crash
# / local DoS. Fixed with `arg.starts_with('-')`.
#
# Each test asserts the SECURE behavior; a FAIL means the bug is back.
#
# Usage:
#   ./test/security/arg_panic.sh
#   AS_TREE_BIN=/path/to/as-tree ./test/security/arg_panic.sh
#
# Exit status: 0 if the panic is resolved, 1 otherwise.

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

# Did the process die from a Rust panic? (debug unwind = 101, abort = 134, or a
# panic message on stderr.)
is_panic() { # <exit_code> <stderr_file>
  [ "$1" = 101 ] && return 0
  [ "$1" = 134 ] && return 0
  grep -qiE 'panicked|char boundary' "$2" && return 0
  return 1
}

echo "Argument-parsing panic (src/options.rs)"

# A1: non-existent file whose name starts with a 2-byte char must NOT panic.
out="$("$BIN" 'ñoño-nonexistent.txt' 2>"$WORK/a1.err" </dev/null)"; code=$?
if is_panic "$code" "$WORK/a1.err"; then
  fail "A1 leading multi-byte arg does not crash" "panicked (exit $code): $(head -1 "$WORK/a1.err")"
else
  pass "A1 leading multi-byte arg does not crash (clean exit $code)"
fi

# A2: same for a 4-byte char (emoji).
out="$("$BIN" '😀-nonexistent.txt' 2>"$WORK/a2.err" </dev/null)"; code=$?
if is_panic "$code" "$WORK/a2.err"; then
  fail "A2 leading emoji arg does not crash" "panicked (exit $code): $(head -1 "$WORK/a2.err")"
else
  pass "A2 leading emoji arg does not crash (clean exit $code)"
fi

# A3: regression -- a genuine unknown flag must still be rejected, not panic.
out="$("$BIN" --bogus 2>"$WORK/a3.err" </dev/null)"; code=$?
if is_panic "$code" "$WORK/a3.err"; then
  fail "A3 unknown flag still rejected cleanly" "panicked (exit $code)"
elif [ "$code" = 1 ] && grep -q 'Unrecognized option' "$WORK/a3.err"; then
  pass "A3 unknown flag still rejected cleanly (exit 1)"
else
  fail "A3 unknown flag still rejected cleanly" "expected exit 1 + 'Unrecognized option', got exit $code"
fi

# A4: positive path -- an EXISTING file with a multi-byte leading name reads OK.
printf 'src/lib\nsrc/bin\n' > "$WORK/ñdata.txt"
out="$( cd "$WORK" && "$BIN" 'ñdata.txt' 2>"$WORK/a4.err" </dev/null )"; code=$?
if is_panic "$code" "$WORK/a4.err"; then
  fail "A4 existing multi-byte-named file reads correctly" "panicked (exit $code)"
elif [ "$code" = 0 ] && printf '%s' "$out" | grep -q 'lib' && printf '%s' "$out" | grep -q 'bin'; then
  pass "A4 existing multi-byte-named file reads correctly (exit 0)"
else
  fail "A4 existing multi-byte-named file reads correctly" "exit $code, output: $out"
fi

echo
if [ "$nfail" -ne 0 ]; then
  echo "$(_r FAIL): $nfail test(s) failing -- argument panic is NOT resolved."
  exit 1
fi
echo "$(_g OK): argument panic resolved."
exit 0
