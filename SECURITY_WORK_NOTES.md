# as-tree — work handoff notes

> Personal memo for when I come back to this project. **Not committed, not part
> of any PR** — it is an untracked local file. Delete it whenever I want.
>
> Last updated: **2026-07-01**

## TL;DR

I did a small security pass on `as-tree`, found two real issues and fixed both,
added a `-L` depth-limit feature, turned my fork into a polished distributable
version (README rewrite, wget installer, release pipeline, first release cut),
and finally **ripped out Bazel** so the fork is Cargo-only.
**Three** PRs are open on upstream (`jez/as-tree`): two security fixes (#27, #28)
and one feature (#29), all **waiting for the maintainer to approve CI** (fork PRs
from a first-time contributor are gated). Nothing to do upstream until the
maintainer looks. Everything lives on my fork's `master` and is installed to
`~/.cargo/bin/as-tree`, so I can use it all today.

- Upstream: `origin` = https://github.com/jez/as-tree (I have **read-only**, `push:false`)
- My fork: `fork` = git@github.com:jvidal86/as-tree.git (where I push)
- Open PRs: **#27** panic fix · **#28** escape injection · **#29** `-L` depth limit
- **I cannot merge the upstream PRs** — only the maintainer can (read-only access).
- **My fork's `master` is the finished version** — all three fixes/feature PLUS
  the docs + distribution work + the Bazel removal (commit `1842d83`). Local
  `master` was fast-forwarded to match and is now well ahead of `origin/master`.
  To pull upstream updates later: `git fetch origin && git merge origin/master`
  (heads-up: upstream still has Bazel, so a merge will bring back the Bazel
  files I deleted — re-delete them, or cherry-pick instead of merge).

---

## What I found (security review of the whole project)

`as-tree` is a small Rust CLI: read a list of paths from stdin/file, print them
as a tree. No `unsafe` code, so memory safety is fine. Findings:

| # | Severity | Issue | Status |
|---|----------|-------|--------|
| 1 | High-ish | **Terminal escape-sequence injection** — input paths printed to the terminal with no sanitization; a crafted filename via `find . \| as-tree` can inject ANSI/OSC/CR escapes (output spoof, window-title/OSC52 clipboard, line rewrite). | **FIXED** → PR #28 |
| 2 | Medium | **Panic on non-ASCII argument** — `&arg[..1]` in `src/options.rs` slices by byte; a multi-byte leading char (`as-tree ñoño.txt`) panics (exit 101 / SIGABRT). Crash / local DoS. | **FIXED** → PR #27 |
| 3 | Low | **Vulnerable/unmaintained deps** — `atty 0.2.14` (RUSTSEC-2021-0145 + unmaintained), `ansi_term 0.12.1` (unmaintained). | **NOT fixed** (backlog) |
| 4 | Low | **`release.sh`** unquoted `$version` interpolated into `sed`. Maintainer-only script. | **FIXED** on fork master (version now validated against `N.N.N` before `sed`) |
| 5 | Info | `src/main.rs` `lines().filter_map(Result::ok)` silently drops non-UTF-8 / error lines; also a clippy warning. | **NOT fixed** (backlog) |
| 6 | Info | **Bazel CI broken repo-wide** — `WORKSPACE` pinned `com_grail_bazel_toolchain` by a stale `sha256` + old `strip_prefix` (GitHub archive drift + repo rename to `toolchains_llvm`). | **N/A on fork** (Bazel removed entirely — see below). Fix still relevant upstream → planned upstream PR. |

---

## What I changed (the two PRs)

### PR #27 — panic fix
- URL: https://github.com/jez/as-tree/pull/27
- Branch: `fix/arg-panic-non-ascii` → base `master`
- Commits:
  - `79da59e` Fix panic on arguments starting with a multi-byte character (`&arg[..1]` → `arg.starts_with('-')`)
  - `aa31fc1` Add regression test `test/security/arg_panic.sh` (+ `test/security/README.md`)
- Files: `src/options.rs`, `test/security/arg_panic.sh`, `test/security/README.md`

### PR #28 — escape-injection fix
- URL: https://github.com/jez/as-tree/pull/28
- Branch: `security/escape-injection` → base `master`
- Commits:
  - `821e364` Add escape-sequence injection test `test/security/escape_injection.sh`
  - `e53a46a` Escape control characters in path output (new `escape_control` in `src/main.rs`, applied before painting + unit tests)
- Files: `src/main.rs`, `test/security/escape_injection.sh`
- Design choices worth remembering (likely review topics):
  - Escapes via `char::is_control()` → `escape_default` (ESC → `\u{1b}`, CR → `\r`…). Printable Unicode (café, 中文, 🦀) untouched.
  - Escaping is **always on**, not TTY-gated, because the attack also works through `as-tree > file; cat file`.
  - Verified output is **byte-identical to master** on all control-free input (fixtures, `-f`, color).

### PR #29 — `-L <n>` depth-limit feature (added later, 2026-07-01)
- URL: https://github.com/jez/as-tree/pull/29
- Branch: `feature/level-depth` → base `master` (squashed to **one** commit)
- What: `-L <n>` prints only the first `<n>` path components (like `tree -L`),
  `n` must be > 0. Implemented by truncating each input path to `n` components
  before inserting into the trie — `print`/`_print`/collapse logic untouched.
- Files: `src/options.rs`, `src/main.rs`, `test/cli/level/{run.sh,run.sh.exp}`
- Semantics chosen: **level = one path component from the top** (matches `tree`).
  Nuance: at the truncation boundary a collapsed chain splits (e.g. `a/b/c`+`a/b/d`
  with `-L 2` shows `a`/`b`, not `a/b`). Documented + tested.
- Independent of #27/#28 (off `master`, different concern).

All three PRs branch off `master` and touch mostly different areas, so they are
independent. (The combined branch below did hit two small merge conflicts —
the shared arg-parse spot and the `mod tests` block — both trivially resolved.)

---

## Distribution & docs (fork `master` only — NOT in any upstream PR)

These point at `jvidal86/as-tree`, so they're fork-specific and live only on my
fork's `master`, not on the PR branches:

- **`README.md`** — rewritten and reorganized: Features / Installation / Usage /
  Examples / Developing (incl. a *Releasing* subsection) / Roadmap. Documents
  `-L` with before/after examples + the collapse-boundary note. Dead Travis
  badge → GitHub Actions badge (points at my fork).
- **`install.sh`** — `wget` quick-installer. Detects OS/arch, downloads the
  latest release tarball `as-tree-<linux|macos>-<x86_64|aarch64>.tar.gz` and
  installs to `/usr/local/bin` (or `$PREFIX/bin`, no sudo). `curl` fallback,
  `VERSION=` override. **Needs a published release to actually work.**
- **`.github/workflows/release.yml`** — on **tag push**, builds the 4 targets
  (linux/macos × x86_64/aarch64; linux-aarch64 is cross-compiled with
  `gcc-aarch64-linux-gnu`) and publishes a GitHub release with those tarballs.
  `workflow_dispatch` = build-only dry run. `fail-fast: false`.
- **`tools/scripts/release.sh`** — adapted from upstream: bumps version in
  `Cargo.toml` + `src/BUILD`, refreshes `Cargo.lock`, regenerates the version
  golden test **with Cargo** (not Bazel), commits, tags, and pushes to `fork`
  (override `REMOTE=`). Validates the version arg → also fixes finding #4.

**Release chain:** `./tools/scripts/release.sh <ver>` → tag → `release.yml`
builds & publishes → users install via the `wget` one-liner.

### First release CUT & VERIFIED — 0.13.0 (2026-07-01)

- Ran `./tools/scripts/release.sh 0.13.0` → commit `e06ffcc`, tag `0.13.0`,
  pushed to `fork`.
- Actions **are** enabled on the fork. The Release workflow **succeeded on all
  4 targets** (incl. the cross-compiled `linux-aarch64` and both macOS — the
  cross-builds I worried about all passed) and published the release:
  https://github.com/jvidal86/as-tree/releases/tag/0.13.0
- **End-to-end verified:** ran `install.sh` for real → it downloaded the
  published `as-tree-linux-x86_64.tar.gz`, installed it, and the binary reports
  `0.13.0` and has `-L`. The `wget … | sh` installer works.
- To cut the next one: `./tools/scripts/release.sh 0.14.0`.

---

## Bazel removed — Cargo-only (2026-07-01)

Decided the Bazel setup wasn't worth it for a 2-file CLI (it had bit-rotted:
stale toolchain `sha256`, then a repo-rename `strip_prefix` break — whack-a-mole
on Bazel 3.1.0 / LLVM 9 / rules_rust 2020 / cargo-raze). Ripped it all out;
Cargo is now the only toolchain.

- **Deleted:** `WORKSPACE`, `.bazelrc`, `./bazel` + `tools/bazel`, all `BUILD`
  files, `test/generate_tests.bzl`, the `run_one_*.sh` drivers, `third_party/`
  (cargo-raze output), `tools/scripts/ci-setup.sh`, the Bazel CI workflow, the
  runfiles-specific `test/cli/` golden tests, dead `.travis.yml`, and the
  `[raze]` block in `Cargo.toml`. (~950 net lines removed.)
- **Added:** `tests/integration.rs` — `cargo test` end-to-end coverage.
  `fixtures` replays every `test/fixture/*.txt` vs its `.exp`; other tests pin
  `-L` / `-f` / `--color` / `--version` / error behavior deterministically.
- **CI:** `.github/workflows/ci.yml` runs `cargo build + test + clippy` + the
  `test/security/*.sh` scripts on linux & macOS, plus an advisory `cargo audit`
  job. **Green on both platforms** (after fixing one flaky-test BrokenPipe race
  in my harness).
- Build `cargo build` · Test `cargo test` · Lint `cargo clippy` · Audit
  `cargo audit` · Release `./tools/scripts/release.sh <ver>`.
- The Bazel `WORKSPACE` fixes (checksum + strip_prefix) are gone from the fork
  but still useful **upstream** — opened as a small PR to `jez` (see below).

---

## Current status (2026-07-01)

- **Three** PRs OPEN: #27 (panic), #28 (escape), #29 (`-L`).
- CI runs are in **`action_required`** — GitHub is waiting for the maintainer to
  approve workflows for my fork PRs. I **cannot** approve this myself (no write
  access to upstream).
  - panic CI run:  https://github.com/jez/as-tree/actions/runs/28468802540
  - escape CI run: https://github.com/jez/as-tree/actions/runs/28468805077
  - `-L` PR #29: check with `gh pr checks 29 --repo jez/as-tree`
- The maintainer may take days/weeks/months. That's fine — nothing to babysit.
- The **fork's own CI (cargo) is green** on linux & macOS. (The upstream PR
  branches still use Bazel — those checks will fail on the toolchain drift, not
  my code; see finding #6 / the upstream WORKSPACE PR.)
- **Fork `master`** has everything merged + Bazel removed, installed to
  `~/.cargo/bin/as-tree`. This is my day-to-day binary. (`security/all-fixes` was
  the intermediate combined branch; `master` has since moved past it.)

---

## How to USE my fork right now (don't need the maintainer)

**Already done:** the combined branch `security/all-fixes` (all three PRs merged)
is built and installed to `~/.cargo/bin/as-tree`. If I ever need to rebuild it:

```bash
cd /home/jvidal/PROJECTS/as-tree
git checkout security/all-fixes
cargo install --path . --force            # installs to ~/.cargo/bin/as-tree
```

Individual branches (one change each) if I want just one: `fix/arg-panic-non-ascii`,
`security/escape-injection`, `feature/level-depth`.

> Note: `security/all-fixes` is a LOCAL convenience branch — not pushed, not a PR.
> To refresh it after changing any of the three branches, re-merge them into it.

---

## What to do when the PRs get reviewed

### 1. Check whether CI ran / passed
```bash
gh pr checks 27 --repo jez/as-tree
gh pr checks 28 --repo jez/as-tree
# or watch live:
gh pr checks 27 --repo jez/as-tree --watch
```

> ⚠️ **Expect the Bazel "Rust" check to FAIL for an infra reason, not my code**
> (finding #6 — the `WORKSPACE` toolchain checksum drift; confirmed on my fork's
> runs). If it's red, read the log: if it's the `com_grail_bazel_toolchain`
> checksum error, that's pre-existing and repo-wide. My code is verified green
> under `cargo`. Fixing the `WORKSPACE` pin (backlog) would be a good companion PR.

### 2. If the maintainer requests changes
Work on the matching local branch, commit, and push to **`fork`** (the PR
updates automatically):
```bash
git checkout fix/arg-panic-non-ascii      # or security/escape-injection
# ...edit, build, test...
cargo build && cargo test
git commit -am "Address review: <what>"
git push fork HEAD
```

### 3. Likely review feedback to anticipate
- **Escaping format** (PR #28): maintainer might prefer `?` (like `ls` default)
  instead of `\u{1b}`, or want it **TTY-gated**. The code is centralized in
  `escape_control()` in `src/main.rs` — easy to change. I already offered both
  options in the PR description.
- **Bazel CI on the PRs will be red** (finding #6, toolchain drift) — infra, not
  my code. The upstream WORKSPACE PR fixes it. My PR branches are output-neutral
  for normal input, so no fixture `.exp` updates needed either way.
- **Clippy**: pre-existing warning at `src/main.rs` `filter_map(Result::ok)`
  (finding #5). NOT mine, left out of scope. Could offer a small PR.
- **NOTE:** the PR branches (#27/#28/#29) still contain Bazel — the Bazel removal
  is only on my fork's `master`, deliberately NOT in the upstream PRs.

### 4. After merge
```bash
git checkout master
git fetch origin
git rebase origin/master           # or: git pull origin master
git branch -d fix/arg-panic-non-ascii security/escape-injection   # delete merged branches
git push fork --delete fix/arg-panic-non-ascii security/escape-injection
```

---

## Backlog (findings I did NOT fix — future PRs if motivated)

- **Drop `atty`** → use `std::io::IsTerminal` (stable since Rust 1.70). Removes
  RUSTSEC-2021-0145 + an unmaintained dep. Touches `src/main.rs` (the two
  `atty::is(...)` calls).
- **Replace `ansi_term`** (unmaintained) → `nu-ansi-term` / `anstyle`.
- **`lines().filter_map(Result::ok)`**: silently drops lines; also the clippy
  lint. Consider `map_while` or reading as bytes (`OsString`) to keep non-UTF-8
  paths.
- ~~Cut the first real release~~ **DONE — 0.13.0** (installer verified).
- ~~Add `cargo audit` to CI~~ **DONE** (advisory job in `ci.yml`).
- ~~Remove Bazel~~ **DONE** (fork is Cargo-only).
- **Publish to crates.io** (the "cargo distribution" — `cargo install as-tree`).
- **Drop `atty` / `ansi_term`** would make the advisory `cargo audit` job clean
  (see finding #3): `atty` → `std::io::IsTerminal`; `ansi_term` → `anstyle` /
  `nu-ansi-term`.

---

## Quick reference

- Build `cargo build` · Test `cargo test` · Lint `cargo clippy` · Audit `cargo audit`
- CI = `.github/workflows/ci.yml` (cargo, linux+macOS) · Release = tag → `release.yml`
- Tests: `tests/integration.rs` (fixtures + CLI behavior) + unit tests in
  `src/main.rs` + `test/security/*.sh` (run by CI).
- Security tests: `./test/security/arg_panic.sh`, `./test/security/escape_injection.sh`
- Distribution files (fork master): `README.md`, `install.sh`,
  `tools/scripts/release.sh`, `.github/workflows/release.yml`
- Cut a release: `./tools/scripts/release.sh <ver>` (pushes to `fork`)
