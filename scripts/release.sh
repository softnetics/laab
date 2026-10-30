#!/usr/bin/env bash
# Release LAAB.app (the Tauri app in desktop/) through GitHub Actions.
#
#   ./scripts/release.sh 0.2.0            # start the Release workflow for v0.2.0
#   ./scripts/release.sh 0.2.0 --draft    # same, but leave the release as a draft
#   ./scripts/release.sh 0.2.0 --zip      # publish a .zip instead of a .dmg
#   ./scripts/release.sh 0.2.0 --dry-run  # test and package here; no workflow
#
# The workflow (.github/workflows/release.yml) tests, builds, signs and
# notarizes on main, writes the changelog with release-notes.sh, then creates
# the tag and the GitHub release together. A dry run tests and runs
# package.sh locally and creates nothing outside dist/.
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { echo "usage: $0 <version> [--draft] [--zip] [--dry-run]" >&2; exit 2; }
die() { echo "release: $*" >&2; exit 1; }

version=""
draft=false
dry_run=false
zip=false
for arg in "$@"; do
  case "$arg" in
    --draft) draft=true ;;
    --zip) zip=true ;;
    --dry-run) dry_run=true ;;
    -h|--help) usage ;;
    -*) usage ;;
    *) [[ -z "$version" ]] || usage; version="${arg#v}" ;;
  esac
done
[[ -n "$version" ]] || usage
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "version must look like 1.2.3 or 1.2.3-beta.1"
tag="v$version"

if $dry_run; then
  [[ "$(uname)" == Darwin ]] || die "must run on macOS"
  command -v pnpm >/dev/null || die "pnpm not found (brew install pnpm)"
  command -v cargo >/dev/null || die "cargo not found (install Rust)"
  [[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"
  (cd desktop && pnpm install --frozen-lockfile && pnpm test)
  (cd desktop/src-tauri && cargo test)
  if $zip; then ./scripts/package.sh "$version" --zip; else ./scripts/package.sh "$version"; fi
  echo "Dry run: skipped tagging and publishing $tag"
  exit 0
fi

command -v gh >/dev/null || die "gh not found (brew install gh)"
gh auth status >/dev/null 2>&1 || die "gh is not logged in (gh auth login)"
[[ "$(git branch --show-current)" == main ]] || die "releases are cut from main"
git fetch --quiet origin main --tags
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "main is not in sync with origin/main"
! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "tag $tag already exists"

gh workflow run release.yml --ref main -f version="$version" -f draft="$draft" -f zip="$zip"
echo "Started the Release workflow for $tag at $(git rev-parse --short HEAD)."
echo "Follow it with: gh run watch \$(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
