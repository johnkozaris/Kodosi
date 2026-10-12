#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

output_dir=${1:?Usage: normalize-cpack-packages.sh <output-dir>}
epoch=${SOURCE_DATE_EPOCH:-$(python3 scripts/release/source-date-epoch.py --source-root .)}
output_dir=$(realpath "$output_dir")
archive="$output_dir/kodosi-0.1.0-linux-x86_64.tar.gz"
test -f "$archive"

stage="$PWD/build/cpack-normalize.$$"
temporary="$output_dir/.kodosi-0.1.0-linux-x86_64.tar.gz.part.$$"
rm -rf "$stage"
rm -f "$temporary"
mkdir -p "$stage"
trap 'rm -rf "$stage"; rm -f "$temporary"' EXIT

tar -xzf "$archive" -C "$stage"
find "$stage" -type d -exec chmod 0755 {} +
while IFS= read -r -d '' path; do
    if [[ -x "$path" ]]; then
        chmod 0755 "$path"
    else
        chmod 0644 "$path"
    fi
done < <(find "$stage" -type f -print0)
find "$stage" -print0 |
    xargs -0 touch -h --date="@$epoch"

tar \
    --sort=name \
    --format=gnu \
    --mtime="@$epoch" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -cf - \
    -C "$stage" kodosi-0.1.0-Linux |
    gzip -n -9 >"$temporary"
mv -f "$temporary" "$archive"
