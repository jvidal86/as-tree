#!/bin/sh
#
# as-tree installer — downloads a prebuilt release binary.
#
#   wget -qO- https://raw.githubusercontent.com/jvidal86/as-tree/master/install.sh | sh
#
# Environment overrides:
#   PREFIX=$HOME/.local   install under $PREFIX/bin (default: /usr/local)
#   VERSION=0.12.0        install a specific release tag (default: latest)
#
# NOTE: this needs a published GitHub release that ships assets named
#   as-tree-<linux|macos>-<x86_64|aarch64>.tar.gz  (each containing the
#   `as-tree` binary at the archive root). Until such a release exists, use one
#   of the "from source" methods in the README.

set -eu

REPO="jvidal86/as-tree"
BIN="as-tree"

# --- detect platform -------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)  os_tag="linux"  ;;
  Darwin) os_tag="macos"  ;;
  *) echo "as-tree: unsupported OS '$os' — install from source instead." >&2; exit 1 ;;
esac
case "$arch" in
  x86_64 | amd64)  arch_tag="x86_64"  ;;
  arm64 | aarch64) arch_tag="aarch64" ;;
  *) echo "as-tree: unsupported arch '$arch' — install from source instead." >&2; exit 1 ;;
esac

asset="${BIN}-${os_tag}-${arch_tag}.tar.gz"
version="${VERSION:-latest}"
if [ "$version" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
  url="https://github.com/${REPO}/releases/download/${version}/${asset}"
fi

# --- pick a downloader (wget, else curl) -----------------------------------
if command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$1" "$2"; }
elif command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL -o "$1" "$2"; }
else
  echo "as-tree: need 'wget' or 'curl' to download." >&2
  exit 1
fi

# --- resolve install location ----------------------------------------------
prefix="${PREFIX:-/usr/local}"
bindir="${prefix}/bin"
sudo=""
if [ "$(id -u)" -ne 0 ] && [ ! -w "$prefix" ] && command -v sudo >/dev/null 2>&1; then
  sudo="sudo"
fi

# --- download, extract, install --------------------------------------------
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "as-tree: downloading ${asset} ..."
fetch "${tmp}/${asset}" "$url"

echo "as-tree: extracting ..."
tar -xzf "${tmp}/${asset}" -C "$tmp"

echo "as-tree: installing to ${bindir} ..."
$sudo mkdir -p "$bindir"
$sudo install -m 0755 "${tmp}/${BIN}" "${bindir}/${BIN}"

echo "as-tree: installed $("${bindir}/${BIN}" --version 2>/dev/null || echo "$BIN")."
case ":${PATH}:" in
  *":${bindir}:"*) ;;
  *) echo "as-tree: note — ${bindir} is not on your PATH." >&2 ;;
esac
