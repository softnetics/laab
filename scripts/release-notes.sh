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
# Commits are "type(scope): subject" (see AGENTS.md). feat, fix and perf get
# their own sections without the prefix; chore, ci, test, refactor, docs,
# build and style are left out, since users don't see them. Anything else,
# such as commits from before the convention, goes under Other changes. A
# scope becomes a bold label in front, named as users know the feature, and
# each section is sorted by label (unlabeled last) so a feature's changes sit
# together. A commit cherry-picked between branches appears twice; keep the
# newest copy.
#
# The scopes, and what the release notes call them, are in scripts/scopes.txt.
scopes=$(grep -v '^#' scripts/scopes.txt | paste -sd ';' -)
# One line per change: section number, label, order in the log, the line.
changes=$(git log --no-merges --format='%s%x09%h' "$previous..HEAD" |
  awk -F '\t' -v scopes="$scopes" '
    function cap(t) { return toupper(substr(t, 1, 1)) substr(t, 2) }
    BEGIN {
      n = split(scopes, pairs, ";")
      for (i = 1; i <= n; i++) { split(pairs[i], kv, "="); names[kv[1]] = kv[2] }
    }
    !seen[$1]++ {
      s = $1; type = ""; scope = ""
      if (match(s, /^[a-z]+(\([^)]*\))?!?: /)) {
        head = substr(s, 1, RLENGTH); s = substr(s, RLENGTH + 1)
        type = head; sub(/[(!:].*/, "", type)
        if (match(head, /\([^)]*\)/)) scope = substr(head, RSTART + 1, RLENGTH - 2)
      }
      if (type ~ /^(chore|ci|test|refactor|docs|build|style)$/) next
      name = scope == "" ? "" : (scope in names ? names[scope] : cap(scope))
      section = type == "feat" ? 1 : type == "fix" ? 2 : type == "perf" ? 3 : 4
      line = "- " (name == "" ? "" : "**" name ":** ") cap(s) " (" $2 ")"
      # "~" sorts after every letter, so unlabeled changes come last.
      printf "%d\t%s\t%06d\t%s\n", section, (name == "" ? "~" : tolower(name)), NR, line
    }
  ' |
  LC_ALL=C sort -t $'\t' -k1,1n -k2,2 -k3,3n |
  awk -F '\t' '
    BEGIN { split("New|Fixed|Performance|Other changes", headings, "|") }
    $1 != last { printf "%s### %s\n\n", (last == "" ? "" : "\n"), headings[$1]; last = $1 }
    { print $4 }
  ')
if [[ -n "$changes" ]]; then
  echo "$changes"
else
  echo "No changes users will notice since $previous."
fi
echo
echo "**Full changelog**: $server/$repo/compare/$previous...$tag"
