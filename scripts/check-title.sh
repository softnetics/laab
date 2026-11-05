#!/usr/bin/env bash
# Check a commit subject or pull request title, or a branch name, against
# the convention in AGENTS.md. Pull requests are squash-merged, so the title
# becomes the commit subject and, for feat, fix and perf, a line in the
# release notes.
#
#   ./scripts/check-title.sh "feat(calendar): open on the day an event card points at"
#   ./scripts/check-title.sh --branch feat/calendar-event-links
set -euo pipefail

cd "$(dirname "$0")/.."

user_types="feat fix perf"
internal_types="refactor test docs chore ci build style"

if [[ "${1:-}" == --branch ]]; then
  branch="${2:-}"
  # Branches GitHub or bots make keep their own names.
  [[ "$branch" != dependabot/* && "$branch" != revert-* ]] || { echo "ok: $branch"; exit 0; }
  re='^([a-z]+)/[a-z0-9]+(-[a-z0-9]+)*$'
  if [[ ! "$branch" =~ $re || " $user_types $internal_types " != *" ${BASH_REMATCH[1]} "* ]]; then
    echo "check-title: branch \"$branch\" is not type/topic" >&2
    echo "  expected: one of $user_types $internal_types, a slash, then a short" >&2
    echo "  lowercase kebab-case topic, like feat/calendar-event-links (AGENTS.md)" >&2
    exit 1
  fi
  echo "ok: $branch"
  exit 0
fi

title="${1:-}"
fail() {
  echo "check-title: $*" >&2
  echo "  title: $title" >&2
  echo "  expected: type(scope): subject, as in AGENTS.md (Commits and branches)" >&2
  exit 1
}

re='^([A-Za-z]+)(\(([^()]+)\))?(!)?: (.+)$'
[[ "$title" =~ $re ]] || fail "not in the form type(scope): subject"
type="${BASH_REMATCH[1]}"
scope="${BASH_REMATCH[3]}"
subject="${BASH_REMATCH[5]}"
[[ "$type$scope" =~ ^[a-z0-9-]*$ ]] || fail "the type and scope are lowercase, like feat(calendar)"

[[ " $user_types $internal_types " == *" $type "* ]] ||
  fail "unknown type \"$type\"; use one of: $user_types $internal_types"

# Changes users see get a release notes label from their scope, so it must be
# one in scripts/scopes.txt. Internal changes may use any scope.
if [[ -n "$scope" && " $user_types " == *" $type "* ]]; then
  scopes=$(sed -n 's/^\([a-z0-9-]*\)=.*/\1/p' scripts/scopes.txt)
  grep -qx -- "$scope" <<<"$scopes" ||
    fail "unknown scope \"$scope\" for $type; use one in scripts/scopes.txt ($(echo $scopes | tr ' ' ',')) or add it there"
fi

[[ "$subject" != *. ]] || fail "the subject ends with a full stop"
[[ "$subject" != " "* ]] || fail "more than one space after the colon"
echo "ok: $title"
