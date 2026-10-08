#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "usage: $0 <signed-path> <expected-team-id> [codesign-command]" >&2
    exit 2
fi

path=$1
expected_team_id=$2
codesign_command=${3:-/usr/bin/codesign}

if [[ ! "$expected_team_id" =~ ^[A-Z0-9]{10}$ ]]; then
    echo "Expected team ID must be a 10-character Apple team identifier." >&2
    exit 1
fi

developer_id_requirement="anchor apple generic and certificate leaf[subject.OU] = \"$expected_team_id\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
"$codesign_command" --verify --strict --verbose=4 -R="$developer_id_requirement" "$path"
details=$("$codesign_command" --display --verbose=4 "$path" 2>&1)

grep -F "Authority=Developer ID Application:" <<<"$details" >/dev/null || {
    echo "$path is not signed by a Developer ID Application certificate." >&2
    exit 1
}
grep -F "TeamIdentifier=$expected_team_id" <<<"$details" >/dev/null || {
    echo "$path is not signed for expected team $expected_team_id." >&2
    exit 1
}
grep -E '^Timestamp=.+' <<<"$details" >/dev/null || {
    echo "$path has no secure signing timestamp." >&2
    exit 1
}
grep -E '^CodeDirectory .*flags=.*\([^)]*runtime[^)]*\)' <<<"$details" >/dev/null || {
    echo "$path is not signed with the hardened runtime." >&2
    exit 1
}
