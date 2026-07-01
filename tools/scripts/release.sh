#!/usr/bin/env bash
#
# Cut a new release: bump the version, commit, tag, and push. Pushing the tag
# triggers the Release workflow (.github/workflows/release.yml), which builds
# and publishes the prebuilt binaries.
#
# Usage:
#   ./tools/scripts/release.sh <new-version>          # e.g. 0.14.0
#   REMOTE=origin ./tools/scripts/release.sh 0.14.0   # override the push remote

set -euo pipefail

version="${1:-}"
remote="${REMOTE:-fork}"

if [ "$version" = "" ]; then
  echo "usage: $0 <new-version>" >&2
  exit 1
fi

# Validate the version shape before splicing it into `sed`. This keeps
# Cargo.toml valid and avoids sed/shell injection via a crafted argument.
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "error: version must look like 1.2.3, got '$version'" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)"

# Bump the [package] version in Cargo.toml (the only line starting with `version = `).
sed -i.bak -e "s/^version = \"[^\"]*\"/version = \"$version\"/" Cargo.toml
rm -f Cargo.toml.bak

# Refresh the lockfile so it records the new package version, and sanity-build.
# (The version is asserted by `tests/integration.rs::version_matches_crate`,
# which reads CARGO_PKG_VERSION, so there is no golden file to regenerate.)
cargo generate-lockfile --offline
cargo build --release

git add Cargo.toml Cargo.lock
git commit -m "Release version $version"
git tag "$version"
git push "$remote" HEAD --tags

echo
echo "Pushed $version to '$remote'. The Release workflow will build and publish the binaries."
