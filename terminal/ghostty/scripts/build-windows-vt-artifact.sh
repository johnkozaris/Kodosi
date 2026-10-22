#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

ROOT_DIR=$(pwd)
SOURCE_DIR=${1:-}
PATCH_APPLICATION_DIGEST=${2:-}
ZIG_STORE_DIGEST=${3:-}

if [ -z "$SOURCE_DIR" ] || [ -z "$PATCH_APPLICATION_DIGEST" ] || [ -z "$ZIG_STORE_DIGEST" ]; then
    echo "Usage: $0 <source_dir> <patch_application_digest> <zig_store_digest>" >&2
    exit 1
fi

# shellcheck disable=SC1091
source Toolchain.env
# shellcheck disable=SC1091
source WindowsVtBuild.env

STAGE_ROOT="$ROOT_DIR/build/windows-vt-stage"
STAGE="$STAGE_ROOT/$GHOSTTY_WINDOWS_VT_LAYOUT"
METADATA="$STAGE_ROOT/$GHOSTTY_WINDOWS_VT_LAYOUT.version"
PREFIX="$STAGE_ROOT/prefix"
CACHE_ROOT="$ROOT_DIR/build/cache"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE/lib" "$STAGE/include"

OPTIONS=(
    "emit-lib-vt=true"
    "target=$GHOSTTY_WINDOWS_VT_TARGET"
    "cpu=$GHOSTTY_WINDOWS_VT_CPU"
    "optimize=$GHOSTTY_WINDOWS_VT_OPTIMIZE"
    "strip=true"
)

echo "[*] building Ghostty VT library for $GHOSTTY_WINDOWS_VT_TARGET"
(
    cd "$SOURCE_DIR"
    env \
        ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$CACHE_ROOT/zig-global}" \
        ZIG_LOCAL_CACHE_DIR="$CACHE_ROOT/vt/$GHOSTTY_WINDOWS_VT_TARGET/zig-local" \
        zig build "${OPTIONS[@]/#/-D}" --system "$SOURCE_DIR/zig-pkg" --prefix "$PREFIX"
)

ARCHIVE="$PREFIX/lib/ghostty-vt-static.lib"
if [ ! -f "$ARCHIVE" ]; then
    echo "[!] expected Ghostty VT static library not found: $ARCHIVE" >&2
    find "$PREFIX" -maxdepth 4 -type f -print | sort >&2
    exit 1
fi

kodosi_windows_env
python3 - "$ARCHIVE" "$STAGE/lib/ghostty-vt-static.lib" "$STAGE_ROOT/objects" "$(cygpath -w "$ROOT_DIR")" <<'PY'
import subprocess
import sys
from pathlib import Path, PureWindowsPath

source, destination, object_root = (Path(argument) for argument in sys.argv[1:4])
build_root = sys.argv[4]
object_root.mkdir(parents=True)

members = [
    PureWindowsPath(member).name
    for member in subprocess.check_output(["llvm-ar", "t", source], text=True).splitlines()
]
if len(members) != len(set(members)):
    raise SystemExit("[!] Ghostty VT archive contains duplicate member basenames")
objects = sorted(member for member in members if member.endswith(".obj"))
if len(objects) != len(members):
    raise SystemExit(f"[!] Ghostty VT archive contains unexpected members: {members}")

subprocess.run(["llvm-ar", "x", source], cwd=object_root, check=True)
for name in objects:
    path = object_root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"[!] Ghostty VT archive contains an empty object: {name}")
    # Stripping renumbers the symbols; llvm-objcopy would leave each address-significance
    # table pointing at the old numbers, so drop the tables and let the linker assume all.
    subprocess.run(["llvm-objcopy", "--strip-debug", "--remove-section=.llvm_addrsig", path], check=True)
    # Clang writes the build time into each COFF header; a zero time gives the
    # same library from the same inputs. A big object header keeps it 8 bytes in.
    data = bytearray(path.read_bytes())
    stamp = 8 if data[:4] == b"\x00\x00\xff\xff" else 4
    data[stamp:stamp + 4] = bytes(4)
    path.write_bytes(data)
subprocess.run(["llvm-lib", "/out:ghostty-vt-static.lib", *objects], cwd=object_root, check=True)
(object_root / "ghostty-vt-static.lib").replace(destination)

if sorted(subprocess.check_output(["llvm-ar", "t", destination], text=True).splitlines()) != objects:
    raise SystemExit("[!] normalized Ghostty VT archive changed its object set")
contents = destination.read_bytes().lower()
for form in (build_root, build_root.replace("\\", "/")):
    if form.lower().encode() in contents:
        raise SystemExit("[!] Ghostty VT archive embeds the build repository path")
PY
rm -rf "$STAGE_ROOT/objects"
cp -R "$PREFIX/include/." "$STAGE/include/"
rm -rf "$PREFIX"

if ! diff -r "$STAGE/include" "$ROOT_DIR/Vendor/GhosttyVt/linux-x86_64/include" >/dev/null; then
    echo "[!] Windows Ghostty VT headers differ from the other platforms" >&2
    exit 1
fi

ARCHIVE_DIGEST=$(python3 "$ROOT_DIR/scripts/digest.py" file "$STAGE/lib/ghostty-vt-static.lib")
cat > "$METADATA" <<METADATA
package_version=$(tr -d '[:space:]' < VERSION)
ghostty_commit=$(tr -d '[:space:]' < WindowsGhostty.ref)
patches_sha256=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/ghostty)
patch_application_sha256=$PATCH_APPLICATION_DIGEST
zig_version=$ZIG_VERSION
zig_target=$ZIG_TARGET
zig_sha256=$ZIG_SHA256
zig_patches_sha256=$(python3 "$ROOT_DIR/scripts/digest.py" artifact Patches/zig)
zig_store_sha256=$ZIG_STORE_DIGEST
ghostty_build_options=$(IFS=,; printf '%s' "${OPTIONS[*]}")
vt_target=$GHOSTTY_WINDOWS_VT_TARGET
vt_cpu=$GHOSTTY_WINDOWS_VT_CPU
vt_optimize=$GHOSTTY_WINDOWS_VT_OPTIMIZE
vt_archive_sha256=$ARCHIVE_DIGEST
METADATA

DESTINATION="$ROOT_DIR/Vendor/GhosttyVt/$GHOSTTY_WINDOWS_VT_LAYOUT"
rm -rf "$DESTINATION" "$DESTINATION.version"
mv "$STAGE" "$DESTINATION"
mv "$METADATA" "$DESTINATION.version"
rm -rf "$STAGE_ROOT"

LOCK_KEY="${GHOSTTY_WINDOWS_VT_LAYOUT//-/_}_archive_sha256"
echo "[*] Windows VT: Vendor/GhosttyVt/$GHOSTTY_WINDOWS_VT_LAYOUT"
echo "[*] record in Ghostty.lock: $LOCK_KEY=$ARCHIVE_DIGEST"
