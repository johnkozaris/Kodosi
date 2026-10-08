#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 "$ROOT/Tests/Tooling/generate-xcode-project-test.py"
PROJECT="$ROOT/KodosiDesktop.xcodeproj/project.pbxproj"
SCRATCH="$ROOT/build/generated-project-verifier-test"
COPY_ROOT="$SCRATCH/repository"

cleanup() {
    rm -rf "$SCRATCH"
}
trap cleanup EXIT
cleanup
mkdir -p "$COPY_ROOT"
for relative_path in project.yml Config Sources Tests Resources Frameworks Packages KodosiDesktop.xcodeproj; do
    cp -R "$ROOT/$relative_path" "$COPY_ROOT/$relative_path"
done
mkdir -p "$COPY_ROOT/scripts/build"
for task in xcode-gen.sh generate-xcode-project.py deployment-target.sh; do
    cp "$ROOT/scripts/build/$task" "$COPY_ROOT/scripts/build/$task"
done
printf '\n' >>"$COPY_ROOT/project.yml"

bash "$ROOT/Tests/Tooling/deployment-target-verifier-test.sh"
bash "$COPY_ROOT/scripts/build/xcode-gen.sh"
GENERATED="$COPY_ROOT/KodosiDesktop.xcodeproj"
if ! cmp -s "$PROJECT" "$GENERATED/project.pbxproj"; then
    echo "Semantically unchanged generation rewrote the tracked Xcode project" >&2
    exit 1
fi
KODOSI_MACOS_DEPLOYMENT_TARGET=$(<"$ROOT/Config/macOSDeploymentTarget") \
    xcodegen generate \
    --spec "$COPY_ROOT/project.yml" \
    --project "$COPY_ROOT" \
    --project-root "$COPY_ROOT" \
    --quiet

plutil -convert json -o "$SCRATCH/tracked-project.json" "$PROJECT"
plutil -convert json -o "$SCRATCH/generated-project.json" "$GENERATED/project.pbxproj"
python3 - "$SCRATCH/tracked-project.json" "$SCRATCH/generated-project.json" <<'PY'
import json
import sys
from pathlib import Path

tracked, generated = (json.loads(Path(path).read_text()) for path in sys.argv[1:])
if tracked != generated:
    raise SystemExit("Tracked Xcode project differs semantically from isolated generation")
PY
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
