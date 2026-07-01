#!/usr/bin/env bash
#
# Cut a new release on the fork: bump the version, regenerate the version
# golden test, commit, tag, and push. Pushing the tag triggers the Release
# workflow (.github/workflows/release.yml), which builds and publishes the
# prebuilt binaries.
#
# Usage:
#   ./tools/scripts/release.sh <new-version>          # e.g. 0.13.0
#   REMOTE=origin ./tools/scripts/release.sh 0.13.0   # override the push remote
#
# Adapted from the upstream Bazel-based script: it uses Cargo instead of Bazel
# and pushes to the fork remote (default `fork`) instead of `origin`.

set -euo pipefail

version="${1:-}"
remote="${REMOTE:-fork}"

if [ "$version" = "" ]; then
  echo "usage: $0 <new-version>" >&2
  exit 1
fi

# Validate the version shape before it is spliced into `sed` below. This both
# keeps Cargo.toml valid and avoids sed/shell injection via a crafted argument.
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "error: version must look like 1.2.3, got '$version'" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)"

# Bump the version in the crate manifest and the Bazel target.
sed -i.bak -e "s/version = \"[^\"]*\"/version = \"$version\"/" Cargo.toml src/BUILD
rm -f Cargo.toml.bak src/BUILD.bak

# Refresh the lockfile so it records the new package version.
cargo generate-lockfile --offline

# Build the new binary and regenerate the `--version` golden test with it.
# (Same trick as the other CLI tests: point `src/as_tree` at the built binary
# just long enough to run the test script, then remove it.)
cargo build --release
cp target/release/as-tree src/as_tree
bash test/cli/version/run.sh > test/cli/version/run.sh.exp
rm -f src/as_tree

git add Cargo.toml Cargo.lock src/BUILD test/cli/version/run.sh.exp
git commit -m "Release version $version"
git tag "$version"
git push "$remote" HEAD --tags

echo
echo "Pushed $version to '$remote'. The Release workflow will build and publish the binaries."
