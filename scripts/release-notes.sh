#!/usr/bin/env bash
# Print the release notes for LAAB <version> as Markdown: how to install it,
# then every commit since the previous release tag. The Release workflow uses
# this as the GitHub release body.
#
#   ./scripts/release-notes.sh 0.2.0          # commits up to HEAD
#   ./scripts/release-notes.sh 0.2.0 --zip    # install note for a .zip
#
# A stable version is compared with the previous stable tag, so 0.2.0 lists
# everything since 0.1.0 even if 0.2.0-beta.1 came in between. A pre-release
# is compared with the previous tag of any kind.
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { echo "usage: $0 <version> [--zip]" >&2; exit 2; }

version=""
format=dmg
for arg in "$@"; do
  case "$arg" in
    --zip) format=zip ;;
    -*) usage ;;
    *) [[ -z "$version" ]] || usage; version="${arg#v}" ;;
  esac
done
[[ -n "$version" ]] || usage
tag="v$version"

remote=$(git config "branch.$(git branch --show-current).remote" || echo origin)
repo=${GITHUB_REPOSITORY:-$(git remote get-url "$remote" | sed -E 's#^(git@github\.com:|https://github\.com/)##; s#\.git$##')}
server=${GITHUB_SERVER_URL:-https://github.com}

exclude=(--exclude "$tag")
[[ "$version" == *-* ]] || exclude+=(--exclude 'v*-*')
previous=$(git describe --tags --abbrev=0 --match 'v[0-9]*' "${exclude[@]}" HEAD 2>/dev/null || true)

if [[ "$format" == dmg ]]; then
  install_step="Open the .dmg and drag LAAB.app into Applications."
else
  install_step="Unzip it and move LAAB.app into Applications."
fi

machine=$(uname -m)
[[ "$machine" == arm64 ]] && machine="Apple silicon"
echo "Requires macOS 11 or later on $machine. Signed with Developer ID and notarized by Apple. $install_step"
echo
echo "## Changes"
echo
if [[ -z "$previous" ]]; then
  echo "The first release. The [README]($server/$repo#readme) describes what LAAB does."
  exit 0
fi
# A commit cherry-picked between branches appears twice; keep the newest copy.
changes=$(git log --no-merges --format='%s%x09%h' "$previous..HEAD" |
  awk -F '\t' '!seen[$1]++ { print "- " $1 " (" $2 ")" }')
if [[ -n "$changes" ]]; then
  echo "$changes"
else
  echo "No changes since $previous."
fi
echo
echo "**Full changelog**: $server/$repo/compare/$previous...$tag"
