#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <Debug|Release|WorkingTreeValidation> <executable-folder> <resources-folder>" >&2
    exit 64
fi
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
# Xcode gives its build phases a PATH without the Rust tools.
export PATH="$HOME/.cargo/bin:$PATH"
MACOSX_DEPLOYMENT_TARGET=$(<"$repository/clients/macos/Config/macOSDeploymentTarget")
export MACOSX_DEPLOYMENT_TARGET
notices=()
if [[ $1 != Debug ]]; then
    notices=(--notices "$3/AgentBrowserNotices")
fi
program=$(python3 "$repository/scripts/build-agent-browser.py" \
    --target aarch64-apple-darwin \
    --output-dir "$2" \
    ${notices[@]+"${notices[@]}"})
if [[ ${CODE_SIGNING_ALLOWED:-YES} == YES && -n ${EXPANDED_CODE_SIGN_IDENTITY:-} ]]; then
    timestamp=--timestamp=none
    if [[ $1 != Debug ]]; then
        timestamp=--timestamp
    fi
    /usr/bin/codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --options runtime "$timestamp" "$program"
fi
