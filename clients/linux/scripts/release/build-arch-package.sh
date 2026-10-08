#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

version=0.1.0
archive=${1:-"build/release/kodosi-${version}-linux-x86_64.tar.gz"}
archive=$(realpath "$archive")
output_dir=${2:-"build/release"}
mkdir -p "$output_dir"
output_dir=$(realpath "$output_dir")
epoch=${SOURCE_DATE_EPOCH:-$(python3 scripts/release/source-date-epoch.py --source-root .)}
package=$(
    SOURCE_DATE_EPOCH="$epoch" \
        ./scripts/release/write-arch-package.sh "$archive" "$output_dir"
)
package=$(realpath "$package")
./scripts/release/verify-packaged-content.sh "$package"

printf "Arch-compatible package built: %s\n" "$package"
