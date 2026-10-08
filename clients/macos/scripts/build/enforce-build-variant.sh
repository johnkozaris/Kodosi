#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MARKER_KEY=KodosiNonPublishableBuild
MARKER_VALUE=working-tree-validation

case "${CONFIGURATION:-}" in
    Release)
        if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
            echo "Use WorkingTreeValidation for uncommitted native dependencies." >&2
            exit 1
        fi
        ;;
    WorkingTreeValidation)
        resources="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH"
        mkdir -p "$resources"
        printf '%s\n' "$MARKER_VALUE" >"$resources/$MARKER_KEY"
        ;;
    Debug)
        if [[ "${KODOSI_WORKING_TREE_VALIDATION:-0}" == 1 ]]; then
            resources="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH"
            mkdir -p "$resources"
            printf '%s\n' "$MARKER_VALUE" >"$resources/$MARKER_KEY"
        fi
        ;;
    *)
        echo "Unsupported Xcode configuration: ${CONFIGURATION:-<unset>}" >&2
        exit 1
        ;;
esac
