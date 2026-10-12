#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="$ROOT/scripts/build/verify-ghostty-link-provenance.py"
WRAPPER="$ROOT/scripts/build/verify-final-ghostty-image.sh"
SCRATCH="$ROOT/build/ghostty-link-provenance-test"
MANIFEST="$SCRATCH/combined-strong.txt"
EXPECTED="$SCRATCH/libghostty.a"
RENAMED="$SCRATCH/renamed-native.a"
BINARY="$SCRATCH/KodosiDesktop"

trap 'rm -rf "$SCRATCH"' EXIT
rm -rf "$SCRATCH"
mkdir -p "$SCRATCH"
: >"$EXPECTED"
: >"$RENAMED"
: >"$BINARY"
printf '%s\n' _ghostty_terminal_new _FT_Done_Face >"$MANIFEST"

write_map() {
    local second=$1
    cat >"$SCRATCH/link.map" <<EOF
# Path: $BINARY
# Object files:
[  1] $EXPECTED(ghostty.o)
[  2] $second(extra.o)
# Symbols:
# Address Size File Name
0x1000 0x10 [  1] _ghostty_terminal_new
0x1010 0x10 [  2] _FT_Done_Face
# Dead Stripped Symbols:
# Size File Name
EOF
}

write_map "$EXPECTED"
python3 "$CHECKER" "$SCRATCH/link.map" "$MANIFEST" "$EXPECTED" "$BINARY" >/dev/null

write_map "$RENAMED"
if python3 "$CHECKER" "$SCRATCH/link.map" "$MANIFEST" "$EXPECTED" "$BINARY" \
    >"$SCRATCH/renamed.log" 2>&1; then
    echo "Ghostty link verifier accepted an authority symbol from a renamed archive." >&2
    exit 1
fi
grep -F "unexpected contributors" "$SCRATCH/renamed.log" >/dev/null
grep -F "$RENAMED(extra.o)" "$SCRATCH/renamed.log" >/dev/null

cat >"$SCRATCH/no-authority.map" <<EOF
# Path: $BINARY
# Object files:
[  1] $EXPECTED(other.o)
# Symbols:
# Address Size File Name
0x1000 0x10 [  1] _unrelated
# Dead Stripped Symbols:
EOF
if python3 "$CHECKER" "$SCRATCH/no-authority.map" "$MANIFEST" "$EXPECTED" "$BINARY" \
    >"$SCRATCH/empty.log" 2>&1; then
    echo "Ghostty link verifier accepted a link map without authority symbols." >&2
    exit 1
fi
grep -F "contains no live Ghostty authority symbols" "$SCRATCH/empty.log" >/dev/null

write_map "$EXPECTED"
if python3 "$CHECKER" "$SCRATCH/link.map" "$MANIFEST" "$EXPECTED" "$SCRATCH/other-binary" \
    >"$SCRATCH/wrong-binary.log" 2>&1; then
    echo "Ghostty link verifier accepted a map for another executable." >&2
    exit 1
fi
grep -F "not $SCRATCH/other-binary" "$SCRATCH/wrong-binary.log" >/dev/null

cat >"$SCRATCH/dead-only.map" <<EOF
# Path: $BINARY
# Object files:
[  1] $EXPECTED(ghostty.o)
# Symbols:
# Address Size File Name
0x1000 0x00 [  0] _ghostty_terminal_new
# Dead Stripped Symbols:
# Size File Name
<<dead>> 0x10 [  1] _ghostty_terminal_new
EOF
if python3 "$CHECKER" "$SCRATCH/dead-only.map" "$MANIFEST" "$EXPECTED" "$BINARY" \
    >"$SCRATCH/dead-only.log" 2>&1; then
    echo "Ghostty link verifier accepted only dead/synthesized authority symbols." >&2
    exit 1
fi
grep -F "contains no live Ghostty authority symbols" "$SCRATCH/dead-only.log" >/dev/null

HASHED_BINARY="$SCRATCH/release/deps/kodosi-deadbeef"
PUBLIC_BINARY="$SCRATCH/release/kodosi"
mkdir -p "$(dirname "$HASHED_BINARY")"
printf 'same Cargo output\n' >"$HASHED_BINARY"
cp "$HASHED_BINARY" "$PUBLIC_BINARY"
cat >"$SCRATCH/cargo-link.map" <<EOF
# Path: $HASHED_BINARY
# Object files:
[  1] $EXPECTED(ghostty.o)
# Symbols:
# Address Size File Name
0x1000 0x10 [  1] _ghostty_terminal_new
# Dead Stripped Symbols:
EOF
map_binary=$(
    LC_ALL=C awk '
      /^# Path: / {
        sub(/^# Path: /, "")
        print
        exit
      }
    ' "$SCRATCH/cargo-link.map"
)
cmp -s "$map_binary" "$PUBLIC_BINARY"
python3 "$CHECKER" \
    "$SCRATCH/cargo-link.map" "$MANIFEST" "$EXPECTED" "$map_binary" >/dev/null
printf 'divergent Cargo output\n' >"$PUBLIC_BINARY"
if cmp -s "$map_binary" "$PUBLIC_BINARY"; then
    echo "Cargo link-output bridge accepted divergent public artifact bytes." >&2
    exit 1
fi

WRAPPER_ROOT="$SCRATCH/wrapper-repository/clients/macos"
WRAPPER_GHOSTTY="$SCRATCH/wrapper-ghostty"
WRAPPER_APP="$WRAPPER_ROOT/build/KodosiDesktop.app"
WRAPPER_BINARY="$WRAPPER_APP/Contents/MacOS/KodosiDesktop"
WRAPPER_ARCHIVE="$WRAPPER_GHOSTTY/Vendor/GhosttyKit.xcframework/macos-arm64/libghostty.a"
WRAPPER_MANIFEST="$WRAPPER_GHOSTTY/Vendor/symbols/combined-strong.txt"
WRAPPER_MAP="$SCRATCH/wrapper-link.map"
WRAPPER_BIN="$SCRATCH/wrapper-bin"
mkdir -p \
    "$WRAPPER_ROOT/scripts/build" \
    "$(dirname "$WRAPPER_BINARY")" \
    "$(dirname "$WRAPPER_ARCHIVE")" \
    "$(dirname "$WRAPPER_MANIFEST")" \
    "$WRAPPER_BIN"
cp "$WRAPPER" "$WRAPPER_ROOT/scripts/build/verify-final-ghostty-image.sh"
cp "$CHECKER" "$WRAPPER_ROOT/scripts/build/verify-ghostty-link-provenance.py"
printf 'locked archive\n' >"$WRAPPER_ARCHIVE"
printf 'Mach-O fixture\n' >"$WRAPPER_BINARY"
chmod +x "$WRAPPER_BINARY"
printf '%s\n' \
    _ghostty_surface_new \
    _ghostty_surface_checkpoint_restore \
    _ghostty_surface_set_display_id \
    _ghostty_terminal_new \
    _ghostty_terminal_vt_write >"$WRAPPER_MANIFEST"
wrapper_sha=$(shasum -a 256 "$WRAPPER_ARCHIVE" | awk '{print $1}')
printf 'combined_archive_sha256=%s\n' "$wrapper_sha" >"$WRAPPER_ROOT/../../Ghostty.lock"
cat >"$WRAPPER_MAP" <<EOF
# Path: $WRAPPER_BINARY
# Object files:
[  1] $WRAPPER_ARCHIVE(ghostty.o)
# Symbols:
# Address Size File Name
0x1000 0x10 [  1] _ghostty_surface_new
0x1010 0x10 [  1] _ghostty_surface_checkpoint_restore
0x1020 0x10 [  1] _ghostty_surface_set_display_id
0x1030 0x10 [  1] _ghostty_terminal_new
0x1040 0x10 [  1] _ghostty_terminal_vt_write
# Dead Stripped Symbols:
# Size File Name
EOF
cat >"$WRAPPER_BIN/lipo" <<'EOF'
#!/usr/bin/env bash
printf 'arm64\n'
EOF
cat >"$WRAPPER_BIN/nm" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$WRAPPER_BIN/otool" <<'EOF'
#!/usr/bin/env bash
cat <<'OUTPUT'
Load command 0
      cmd LC_BUILD_VERSION
    minos 26.4
OUTPUT
EOF
chmod +x "$WRAPPER_BIN/lipo" "$WRAPPER_BIN/nm" "$WRAPPER_BIN/otool"
KODOSI_GHOSTTY_DIR="$WRAPPER_GHOSTTY" \
KODOSI_LIPO="$WRAPPER_BIN/lipo" \
KODOSI_NM="$WRAPPER_BIN/nm" \
KODOSI_OTOOL="$WRAPPER_BIN/otool" \
    "$WRAPPER_ROOT/scripts/build/verify-final-ghostty-image.sh" \
    "$WRAPPER_BINARY" "$WRAPPER_MAP" 26.4 >/dev/null

grep -v '_ghostty_surface_set_display_id' "$WRAPPER_MAP" >"$WRAPPER_MAP.missing-display-id"
if KODOSI_GHOSTTY_DIR="$WRAPPER_GHOSTTY" \
    KODOSI_LIPO="$WRAPPER_BIN/lipo" \
    KODOSI_NM="$WRAPPER_BIN/nm" \
    KODOSI_OTOOL="$WRAPPER_BIN/otool" \
    "$WRAPPER_ROOT/scripts/build/verify-final-ghostty-image.sh" \
    "$WRAPPER_BINARY" "$WRAPPER_MAP.missing-display-id" 26.4 \
    >"$SCRATCH/missing-display-id.log" 2>&1; then
    echo "Final Ghostty verifier accepted a binary without display-ID routing." >&2
    exit 1
fi
grep -F 'missing required Ghostty symbol _ghostty_surface_set_display_id' \
    "$SCRATCH/missing-display-id.log" >/dev/null

printf 'Verified complete Ghostty link contributor provenance.\n'
