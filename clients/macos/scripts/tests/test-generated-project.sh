#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROJECT="$ROOT/KodosiDesktop.xcodeproj/project.pbxproj"
SCRATCH="$ROOT/build/test-generated-project"
COPY_ROOT="$SCRATCH/clients/macos"

cleanup() {
    rm -rf "$SCRATCH"
}
trap cleanup EXIT
cleanup
mkdir -p "$COPY_ROOT"
for relative_path in project.yml Config Sources Tests Resources KodosiDesktop.xcodeproj; do
    cp -R "$ROOT/$relative_path" "$COPY_ROOT/$relative_path"
done
cp -R "$ROOT/../apple" "$SCRATCH/clients/apple"
cp "$ROOT/../../VERSION" "$SCRATCH/VERSION"
mkdir -p "$COPY_ROOT/scripts/build"
for task in generate-xcode-project.sh generate-xcode-project.py; do
    cp "$ROOT/scripts/build/$task" "$COPY_ROOT/scripts/build/$task"
done
printf '\n' >>"$COPY_ROOT/project.yml"

bash "$COPY_ROOT/scripts/build/generate-xcode-project.sh"
GENERATED="$COPY_ROOT/KodosiDesktop.xcodeproj"
if ! cmp -s "$PROJECT" "$GENERATED/project.pbxproj"; then
    echo "Semantically unchanged generation rewrote the tracked Xcode project" >&2
    exit 1
fi
for relative_path in \
    project.xcworkspace/contents.xcworkspacedata \
    xcshareddata/xcschemes/KodosiDesktop.xcscheme \
    xcshareddata/xcschemes/WorkingTreeValidation.xcscheme; do
    if ! cmp -s "$ROOT/KodosiDesktop.xcodeproj/$relative_path" "$GENERATED/$relative_path"; then
        diff -u "$ROOT/KodosiDesktop.xcodeproj/$relative_path" "$GENERATED/$relative_path" >&2 || true
        echo "Tracked Xcode project differs from isolated generation: $relative_path" >&2
        exit 1
    fi
done

printf 'Verified isolated Xcode project regeneration and scheme parity.\n'
