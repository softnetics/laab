#!/usr/bin/env bash
# Set LAAB's version in desktop/package.json (which tauri.conf.json reads),
# desktop/src-tauri/Cargo.toml and its Cargo.lock entry. The Release workflow
# runs it before building, so the files always match the released version.
#
#   ./scripts/bump-version.sh 0.3.0
set -euo pipefail

cd "$(dirname "$0")/.."

version="${1:-}"
version="${version#v}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] ||
  { echo "usage: $0 <version>, like 1.2.3 or 1.2.3-beta.1" >&2; exit 2; }

# Only the first "version" key in package.json and in Cargo.toml's [package],
# and the version line right after name = "laab" in Cargo.lock.
V="$version" perl -0pi -e 's/("version"\s*:\s*")[^"]*"/$1$ENV{V}"/' desktop/package.json
V="$version" perl -0pi -e 's/(\[package\][^\[]*?\nversion = ")[^"]*"/$1$ENV{V}"/' desktop/src-tauri/Cargo.toml
V="$version" perl -0pi -e 's/(\nname = "laab"\nversion = ")[^"]*"/$1$ENV{V}"/' desktop/src-tauri/Cargo.lock

for f in desktop/package.json desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock; do
  grep -q "\"$version\"" "$f" || { echo "bump-version: $f does not have $version" >&2; exit 1; }
done
echo "Set the version to $version"
