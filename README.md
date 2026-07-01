# as-tree

[![CI](https://github.com/jvidal86/as-tree/actions/workflows/rust.yml/badge.svg)](https://github.com/jvidal86/as-tree/actions/workflows/rust.yml)

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

### From source (Bazel)

```shell
git clone https://github.com/jvidal86/as-tree
cd as-tree
make install
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

Running the tests requires Bazel. The `./bazel` shell script in this repo will
download and cache a specific version of Bazel for you; Bazel then installs
everything else it needs (including a Rust toolchain).

```shell
# Run the tests:
./bazel test --test_output=errors //test

# Update the golden test output after an intentional change:
./bazel test //test:update
```

Tests come in two flavors:

- **Fixture tests** (`test/fixture/foo.txt` + `foo.txt.exp`) — the `.txt` is the
  input fed to `as-tree`, the `.exp` is the expected output.
- **CLI tests** (`test/cli/<name>/run.sh` + `run.sh.exp`) — a shell script that
  invokes `as-tree` with flags (this is how `-L`, `-f`, and `--color` are
  covered), diffed against its `.exp`.

When you add a dependency, regenerate the Bazel build files:

```shell
cargo install cargo-raze   # one-time setup
cd third_party/cargo
cargo raze
```

### Releasing

Pushing a tag builds the prebuilt binaries and publishes them as a GitHub
release (see [`.github/workflows/release.yml`](.github/workflows/release.yml)),
which is what the quick installer downloads:

```shell
git tag 0.13.0
git push origin 0.13.0
```

The workflow builds `linux`/`macos` × `x86_64`/`aarch64` and uploads the
`as-tree-<os>-<arch>.tar.gz` assets.

## Roadmap

- [ ] Publish to crates.io (`cargo install as-tree`).
- [ ] Publish prebuilt release binaries (so the quick installer works out of the box).
- [ ] Only use box-drawing characters when the locale supports them (`LC_CTYPE=C`).
- [ ] A `-0` flag for NUL-separated input, to support paths containing newlines.

## Acknowledgements

Inspired by [this `fd` feature request](https://github.com/sharkdp/fd/issues/283).

## License

See [LICENSE.md](LICENSE.md).
