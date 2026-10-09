#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <maximum-deployment-target> <architecture>" >&2
    exit 64
fi
maximum_deployment_target=$1
architecture=$2
project_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
scratch_directory=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-archive-verifier.XXXXXX")
trap 'rm -rf "$scratch_directory"' EXIT
verifier="$project_directory/scripts/build/verify-rust-archive.sh"
source "$project_directory/scripts/build/kodosi-paths.sh"
runtime_root=$(kodosi_runtime_root "$project_directory")
header="$project_directory/Frameworks/KodosiKit.xcframework/Headers/kodosi_runtime.h"

python3 - "$header" "$scratch_directory" <<'PY'
import pathlib
import re
import sys
header = pathlib.Path(sys.argv[1]).read_text()
root = pathlib.Path(sys.argv[2])
symbols = sorted(set(re.findall(r'\b(kodosi_[A-Za-z0-9_]+)\s*\(', header)))
vt = ['ghostty_terminal_new', 'ghostty_terminal_vt_write',
      'ghostty_terminal_resize', 'ghostty_checkpoint_encode_buf',
      'ghostty_checkpoint_restore']
for case in ['current', 'stale-protocol', 'stale-abi', 'no-vt', 'bundled-ghostty', 'partial', 'extra']:
    lines = []
    for symbol in symbols:
        if case == 'partial' and symbol == 'kodosi_terminal_input':
            continue
        if symbol == 'kodosi_protocol_version':
            lines.append(f'unsigned int {symbol}(void) {{ return {51 if case == "stale-protocol" else 52}; }}')
        elif symbol == 'kodosi_abi_version':
            lines.append(f'unsigned int {symbol}(void) {{ return {6 if case == "stale-abi" else 7}; }}')
        else:
            lines.append(f'void {symbol}(void) {{}}')
    if case != 'no-vt':
        lines += [f'extern void {symbol}(void);' for symbol in vt]
        lines.append('void vt_authority_probe(void) {' + ''.join(f'{symbol}();' for symbol in vt) + '}')
    if case == 'bundled-ghostty':
        lines.append('void ghostty_terminal_new(void) {}')
    if case == 'extra':
        lines.append('void kodosi_obsolete_lane(void) {}')
    (root / f'{case}.c').write_text('\n'.join(lines) + '\n')
PY

for fixture in "$scratch_directory"/*.c; do
    xcrun --sdk macosx clang -O2 -arch "$architecture" \
        "-mmacosx-version-min=$maximum_deployment_target" \
        -c "$fixture" -o "${fixture%.c}.o"
    libtool -static -o "${fixture%.c}.a" "${fixture%.c}.o"
done
python3 - "$scratch_directory" "$header" <<'PYBUNDLE'
import hashlib
import json
import shutil
import sys
from pathlib import Path
root, header = map(Path, sys.argv[1:])
for archive in root.glob("*.a"):
    bundle = root / archive.stem
    (bundle / "include").mkdir(parents=True)
    shutil.copyfile(archive, bundle / "libkodosi_ffi_c.a")
    shutil.copyfile(header, bundle / "include/kodosi_runtime.h")
    metadata = {"schemaVersion": 1, "target": "aarch64-apple-darwin", "profile": "release",
        "archiveSha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
        "headerSha256": hashlib.sha256(header.read_bytes()).hexdigest()}
    (bundle / "artifact.json").write_text(json.dumps(metadata))
PYBUNDLE

"$verifier" "$scratch_directory/current" "$maximum_deployment_target" "$architecture"

reject() {
    local fixture=$1
    local expected=$2
    if "$verifier" "$scratch_directory/$fixture" "$maximum_deployment_target" "$architecture" \
        >"$scratch_directory/$fixture.log" 2>&1; then
        echo "Verifier accepted $fixture." >&2
        exit 1
    fi
    if ! grep -Fq "$expected" "$scratch_directory/$fixture.log"; then
        cat "$scratch_directory/$fixture.log" >&2
        echo "Verifier rejected $fixture for the wrong reason." >&2
        exit 1
    fi
}

reject stale-protocol "does not return exact desktop/runtime protocol v52"
reject stale-abi "does not return exact C ABI v7"
reject no-vt "lacks required libghostty-vt authority import"
reject bundled-ghostty "bundles Ghostty definitions"
reject partial "public kodosi_* exports differ"
reject extra "public kodosi_* exports differ"

conflicting_runtime="$scratch_directory/conflicting-runtime"
mkdir -p "$conflicting_runtime/target/include"
printf 'void kodosi_conflicting_header(void);\n' >"$conflicting_runtime/target/include/kodosi_runtime.h"
if KODOSI_RUST_DIR="$conflicting_runtime" \
    KODOSI_RUST_HEADER="$conflicting_runtime/target/include/kodosi_runtime.h" \
    "$verifier" "$scratch_directory/partial" "$maximum_deployment_target" "$architecture" \
        >"$scratch_directory/conflicting.log" 2>&1; then
    echo "Verifier accepted independent runtime/header path overrides." >&2
    exit 1
fi
grep -Fq "public kodosi_* exports differ from $scratch_directory/partial/include/kodosi_runtime.h" "$scratch_directory/conflicting.log"
printf '\n' >>"$scratch_directory/current/include/kodosi_runtime.h"
if "$verifier" "$scratch_directory/current" "$maximum_deployment_target" "$architecture" >"$scratch_directory/mismatch.log" 2>&1; then
    echo "Verifier accepted an archive/header association mismatch." >&2
    exit 1
fi
grep -Fq "FFI artifact bundle association mismatch" "$scratch_directory/mismatch.log"
echo "Verified exact ABI7/protocol50 exports, version values, VT imports, and artifact-bound header selection."
