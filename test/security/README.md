# Security regression tests

End-to-end tests that reproduce specific security findings and verify whether
they are resolved. Each test asserts the **secure** behavior, so a **failing**
test means the issue is **still present**.

```sh
./test/security/arg_panic.sh                              # builds with cargo, then tests
AS_TREE_BIN=/path/to/as-tree ./test/security/arg_panic.sh # test a prebuilt binary
```

A script exits `0` only when its finding is resolved. These are standalone
scripts and are intentionally **not** wired into `bazel test //test`.

## `arg_panic.sh` — argument-parsing panic (resolved)

`src/options.rs` checked `&arg[..1] == "-"`, which slices the argument by
**byte**. An argument whose first character is multi-byte UTF-8 (e.g.
`as-tree ñoño.txt`) put byte index 1 mid-character and **panicked** before the
file was ever opened (exit 101, or SIGABRT under the release `panic = "abort"`
profile) — a crash / local DoS.

Fixed with `arg.starts_with('-')`. Tests A1–A4 cover multi-byte and emoji
leading args, the unknown-flag regression, and an existing multi-byte-named
file.
