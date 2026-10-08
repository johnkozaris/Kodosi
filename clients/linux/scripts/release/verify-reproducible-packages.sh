#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

allow_dirty=false
if [[ ${1:-} == "--allow-dirty-client" ]]; then
    allow_dirty=true
elif [[ $# -ne 0 ]]; then
    echo "Usage: verify-reproducible-packages.sh [--allow-dirty-client]" >&2
    exit 2
fi

epoch_arguments=(--source-root .)
if [[ "$allow_dirty" == false ]]; then
    epoch_arguments+=(--require-clean)
fi
epoch=$(python3 scripts/release/source-date-epoch.py "${epoch_arguments[@]}")
python3 scripts/release/verify-release-sources.py --source-root . >/dev/null

cmake="$PWD/.tools/cmake-4.4.3/bin/cmake"
release="$PWD/build/release"
scratch="$PWD/build/package-repeatability"
deb=kodosi_0.1.0_amd64.deb
tgz=kodosi-0.1.0-linux-x86_64.tar.gz
arch=kodosi-bin-0.1.0-1-x86_64.pkg.tar.zst
rm -rf "$scratch"
mkdir -p "$scratch/first" "$scratch/second"

package_cpack() {
    local destination=$1
    umask 022
    SOURCE_DATE_EPOCH="$epoch" \
        "$cmake" --build "$release" --target package
    SOURCE_DATE_EPOCH="$epoch" \
        ./scripts/release/normalize-cpack-packages.sh "$release"
    cp "$release/$deb" "$destination/$deb"
    cp "$release/$tgz" "$destination/$tgz"
}

package_cpack "$scratch/first"
package_cpack "$scratch/second"
cmp "$scratch/first/$deb" "$scratch/second/$deb"
cmp "$scratch/first/$tgz" "$scratch/second/$tgz"

SOURCE_DATE_EPOCH="$epoch" \
    ./scripts/release/write-arch-package.sh \
    "$scratch/first/$tgz" "$scratch/first" >/dev/null
SOURCE_DATE_EPOCH="$epoch" \
    ./scripts/release/write-arch-package.sh \
    "$scratch/second/$tgz" "$scratch/second" >/dev/null
cmp "$scratch/first/$arch" "$scratch/second/$arch"

for pass in first second; do
    for artifact in "$deb" "$tgz" "$arch"; do
        python3 scripts/release/verify-reproducible-archive.py \
            --artifact "$scratch/$pass/$artifact" \
            --source-date-epoch "$epoch"
    done
done

cp "$scratch/second/$deb" "$release/$deb"
cp "$scratch/second/$tgz" "$release/$tgz"
cp "$scratch/second/$arch" "$release/$arch"
sha256sum "$release/$deb" "$release/$tgz" "$release/$arch"
echo "DEB, TGZ, and Arch packages are reproducible from the same build tree"
