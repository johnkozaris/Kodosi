#!/usr/bin/env bash
# Builds the view bundle of live documents, when its sources are newer than the bundle.
set -euo pipefail

views=$(cd "$(dirname "${BASH_SOURCE[0]}")/../documents/views" && pwd)
built="$views/dist/index.html"
if [[ -f $built ]] && [[ -z $(find "$views" -path '*/node_modules' -prune -o -path "$views/dist" -prune -o -type f -newer "$built" -print -quit) ]]; then
    exit 0
fi
bun install --cwd "$views" --frozen-lockfile
bun run --cwd "$views" build
