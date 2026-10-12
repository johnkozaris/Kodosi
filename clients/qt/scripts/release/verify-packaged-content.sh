#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/../.."

artifact=${1:?Usage: verify-packaged-content.sh <deb|tar.gz|pkg.tar.zst>}
artifact=$(realpath "$artifact")
stage="$PWD/build/package-content-verify"
rm -rf "$stage"
mkdir -p "$stage"
trap 'rm -rf "$stage"' EXIT

case "$(basename "$artifact")" in
    *.deb)
        dpkg-deb -x "$artifact" "$stage"
        root="$stage"
        ;;
    *.tar.gz)
        tar -xzf "$artifact" -C "$stage"
        root=$(find "$stage" -mindepth 1 -maxdepth 1 -type d -print -quit)
        test -n "$root"
        ;;
    *.pkg.tar.zst)
        tar --zstd -xf "$artifact" -C "$stage"
        root="$stage"
        ;;
    *)
        echo "Unsupported package artifact: $(basename "$artifact")" >&2
        exit 2
        ;;
esac

./scripts/release/verify-installed-package-content.sh "$root"

echo "Packaged CLI and notices verified: $(basename "$artifact")"
