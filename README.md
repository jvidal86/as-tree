# as-tree

[![CI](https://github.com/jvidal86/as-tree/actions/workflows/ci.yml/badge.svg)](https://github.com/jvidal86/as-tree/actions/workflows/ci.yml)

**Print a list of paths as a tree of paths.**

`as-tree` reads a flat list of file paths — from `stdin` or a file — and renders
it as a tree. It is similar in spirit to [`tree`](https://linux.die.net/man/1/tree),
but instead of walking the filesystem itself it consumes the output of `find`,
`fd`, `git`, `rg --files`, or any command that emits paths. That means **you**
decide exactly which files show up.

Given this input:

```
dir1/foo.txt
dir1/bar.txt
dir2/qux.txt
```

`as-tree` prints:

```
.
├── dir1
│   ├── foo.txt
│   └── bar.txt
└── dir2
    └── qux.txt
```

## Features

- **Reads from any tool** that produces a list of paths (`find`, `fd`, `git ls-files`, …).
- **Depth limiting** with `-L <n>`, just like `tree -L`.
- **Full-path mode** with `-f`.
- **Colorized output** that honors your `LS_COLORS`.
- Single, dependency-light binary written in Rust.

## Installation

### Quick install (prebuilt binary)

```shell
wget -qO- https://raw.githubusercontent.com/jvidal86/as-tree/master/install.sh | sh
```

This downloads the latest release binary for your platform and installs it to
`/usr/local/bin`. To install somewhere else (no `sudo` needed), set `PREFIX`:

```shell
wget -qO- https://raw.githubusercontent.com/jvidal86/as-tree/master/install.sh | PREFIX="$HOME/.local" sh
```

> **Note:** the quick installer needs a published GitHub release. If none is
> available yet, use one of the from-source methods below.

### From source (Cargo)

```shell
cargo install -f --git https://github.com/jvidal86/as-tree
```

Or from a local clone:

```shell
git clone https://github.com/jvidal86/as-tree
cd as-tree
make install    # == cargo install --path .
```

## Usage

```
❯ as-tree --help
Print a list of paths as a tree of paths.

Usage:
  as-tree [options] [<filename>]

Arguments:
  <filename>        The file to read from. When omitted, reads from stdin.

Options:
  --color (always|auto|never)
                    Whether to colorize the output [default: auto]
  -f                Prints the full path prefix for each file.
  -L <level>        Descend only <level> levels deep (must be > 0)
  -h, --help        Print this help message
  -v, --version     Print the version and exit

Example:
  find . -name '*.txt' | as-tree
```

## Examples

### Pipe from `find` or `fd`

`as-tree` shines with tools like `fd` that can prune the file list better than
`tree` can on its own:

```shell
fd --exclude test | as-tree
```

### Limit the depth with `-L`

Use `-L <n>` to show only the first `n` levels, counted from the top. For this
input:

```
docs/guide.md
src/main.rs
src/options.rs
test/cli/run.sh
test/fixture/a.txt
```

`as-tree -L 1` shows just the top level:

```
.
├── docs
├── src
└── test
```

`as-tree -L 2` shows two levels:

```
.
├── docs
│   └── guide.md
├── src
│   ├── main.rs
│   └── options.rs
└── test
    ├── cli
    └── fixture
```

…and without `-L`, the full tree is printed.

> **Note on directory collapsing:** `as-tree` normally collapses a chain of
> single directories onto one line (e.g. `a/b/c`). When `-L` cuts such a chain,
> the boundary directory is shown on its own line instead — so `a/b/c` + `a/b/d`
> with `-L 2` prints `a` then `b`, rather than `a/b`.

## Developing

Everything is driven by Cargo — build, test, lint, and audit:

```shell
cargo build            # build
cargo test             # run all tests
cargo clippy           # lint
cargo audit            # check dependencies for advisories (cargo install cargo-audit)
```

Tests live in two places:

- **`tests/integration.rs`** — end-to-end tests that run the built binary.
  `fixtures` replays every `test/fixture/<name>.txt` and compares stdout to the
  checked-in `<name>.txt.exp` golden file; the rest pin specific CLI behavior
  (`-L`, `-f`, `--color`, `--version`, error handling) with deterministic input.
- **`test/security/*.sh`** — standalone security regression scripts
  (`arg_panic.sh`, `escape_injection.sh`) that build via Cargo and assert the
  hardened behavior. CI runs them too.

To add a fixture, drop `test/fixture/foo.txt` and its expected output
`test/fixture/foo.txt.exp`; the `fixtures` test picks it up automatically.

### Releasing

`./tools/scripts/release.sh <version>` bumps the version, commits, tags, and
pushes to the fork. Pushing the tag triggers
[`.github/workflows/release.yml`](.github/workflows/release.yml), which builds
`linux`/`macos` × `x86_64`/`aarch64` and uploads the `as-tree-<os>-<arch>.tar.gz`
assets that the quick installer downloads.

```shell
./tools/scripts/release.sh 0.14.0
```

## Roadmap

- [x] Prebuilt release binaries + `wget` installer.
- [ ] Publish to crates.io (`cargo install as-tree`).
- [ ] Only use box-drawing characters when the locale supports them (`LC_CTYPE=C`).
- [ ] A `-0` flag for NUL-separated input, to support paths containing newlines.

## Acknowledgements

Inspired by [this `fd` feature request](https://github.com/sharkdp/fd/issues/283).

## License

See [LICENSE.md](LICENSE.md).
