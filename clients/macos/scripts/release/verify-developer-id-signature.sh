#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <signed-path> <expected-team-id>" >&2
    exit 2
fi

path=$1
expected_team_id=$2

developer_id_requirement="anchor apple generic and certificate leaf[subject.OU] = \"$expected_team_id\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
/usr/bin/codesign --verify --strict --verbose=4 -R="$developer_id_requirement" "$path"
details=$(/usr/bin/codesign --display --verbose=4 "$path" 2>&1)

grep -E '^Timestamp=.+' <<<"$details" >/dev/null || {
    echo "$path has no secure signing timestamp." >&2
    exit 1
}
grep -E '^CodeDirectory .*flags=.*\([^)]*runtime[^)]*\)' <<<"$details" >/dev/null || {
    echo "$path is not signed with the hardened runtime." >&2
    exit 1
}
