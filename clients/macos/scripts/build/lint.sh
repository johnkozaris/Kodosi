#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
(
    swiftformat Sources Tests Packages/KodosiTerminal/Sources Packages/KodosiTerminal/Tests \
        --lint --swiftversion 6.0
)
(
    swiftlint lint \
        --quiet \
        Sources Tests Packages/KodosiTerminal/Sources Packages/KodosiTerminal/Tests
)
