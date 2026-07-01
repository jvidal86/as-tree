#!/usr/bin/env bash

set -euo pipefail

# Fixed, deterministic input (piped, not `find`) so the golden output is stable
# across machines. Covers the depth limit at, below, and at/above the real depth.
input='docs/guide.md
src/bin/main.rs
src/lib/a.rs
src/lib/b.rs'

echo ----- -L 1 -----
printf '%s\n' "$input" | src/as_tree -L 1 --color never

echo ----- -L 2 -----
printf '%s\n' "$input" | src/as_tree -L 2 --color never

echo ----- -L 3 -----
printf '%s\n' "$input" | src/as_tree -L 3 --color never

# Collapse boundary: truncation turns a collapsed chain into separate lines.
echo ----- -L 2 collapse boundary -----
printf 'a/b/c\na/b/d\n' | src/as_tree -L 2 --color never