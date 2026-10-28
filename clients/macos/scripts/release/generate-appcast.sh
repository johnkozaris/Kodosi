#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "usage: $0 [--critical] <Kodosi-X.Y.Z.dmg> <archives-folder>" >&2
    exit 64
}
critical=0
if [[ ${1:-} == --critical ]]; then
    critical=1
    shift
fi
[[ $# -eq 2 && $1 == *.dmg ]] || usage
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dmg=$1
archives=$2
version=$(<"$ROOT/../../VERSION")
if [[ $(basename "$dmg") != "Kodosi-$version.dmg" ]]; then
    echo "$dmg is not the disk image of version $version." >&2
    exit 1
fi
packages="$ROOT/build/packages"
xcodebuild -resolvePackageDependencies -project "$ROOT/KodosiDesktop.xcodeproj" -scheme KodosiDesktop \
    -clonedSourcePackagesDirPath "$packages" </dev/null >/dev/null
mkdir -p "$archives"
cp "$dmg" "$archives/"
if [[ -f ${dmg%.dmg}.md ]]; then
    cp "${dmg%.dmg}.md" "$archives/"
fi
arguments=(
    --download-url-prefix "https://github.com/johnkozaris/Kodosi/releases/download/v$version/"
    --maximum-versions 1
    --phased-rollout-interval 86400
    --maximum-deltas 3
    --embed-release-notes
)
if ((critical)); then
    arguments+=(--critical-update-version "")
fi
if [[ ! -t 0 ]]; then
    arguments+=(--ed-key-file -)
fi
"$packages/artifacts/sparkle/Sparkle/bin/generate_appcast" "${arguments[@]}" "$archives"
