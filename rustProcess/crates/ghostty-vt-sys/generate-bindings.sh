#!/bin/sh
set -eu




: "${KODOSI_GHOSTTY_DIR:?set KODOSI_GHOSTTY_DIR to the authoritative kodosi-ghostty checkout}"

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
header="$KODOSI_GHOSTTY_DIR/Vendor/GhosttyVt/macos-arm64/include/ghostty/vt.h"
include_dir="$KODOSI_GHOSTTY_DIR/Vendor/GhosttyVt/macos-arm64/include"
output="$crate_dir/src/bindings.rs"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-bindgen.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

[ -f "$header" ] || {
  printf 'missing authoritative VT header: %s\n' "$header" >&2
  exit 1
}

cargo install \
  --quiet \
  --locked \
  --root "$tmp_dir/tool" \
  --version 0.72.1 \
  bindgen-cli

"$tmp_dir/tool/bin/bindgen" "$header" \
  --disable-header-comment \
  --no-doc-comments \
  --output "$tmp_dir/bindings.rs" \
  --formatter rustfmt \
  --with-derive-default \
  --rust-target 1.82 \
  --blocklist-type '^GhosttySnapshot.*' \
  --blocklist-function '^ghostty_snapshot_.*' \
  --allowlist-function '^(ghostty_checkpoint_(schema|encode_buf|restore)|ghostty_focus_encode|ghostty_formatter_(format_buf|free|terminal_new)|ghostty_key_encoder_(encode|free|new|setopt_from_terminal)|ghostty_key_event_(free|new|set_action|set_key|set_mods|set_unshifted_codepoint|set_utf8)|ghostty_terminal_(compress|compression_activity|free|get|get_multi|new|resize|set|vt_write))$' \
  --allowlist-type '^(GhosttyAllocator|GhosttyBuffer|GhosttyCheckpointEncodeOptions|GhosttyCheckpointInfo|GhosttyCheckpointLimits|GhosttyCheckpointRestoreOptions|GhosttyClipboardContent|GhosttyClipboardLocation|GhosttyClipboardWrite|GhosttyClipboardWriteResult|GhosttyColorScheme|GhosttyDeviceAttributes|GhosttyFocusEvent|GhosttyFormatter|GhosttyFormatterFormat|GhosttyFormatterScreenExtra|GhosttyFormatterTerminalExtra|GhosttyFormatterTerminalOptions|GhosttyKey|GhosttyKeyAction|GhosttyKeyEncoder|GhosttyKeyEvent|GhosttyMods|GhosttySizeReportSize|GhosttyString|GhosttyTerminal|GhosttyTerminalCompressionMode|GhosttyTerminalCompressionResult|GhosttyTerminalData|GhosttyTerminalDesktopNotification|GhosttyTerminalModeConfig|GhosttyTerminalOption|GhosttyTerminalProgressReport|GhosttyTerminalScreen)$' \
  --allowlist-var '^(GHOSTTY_CHECKPOINT_.*|GHOSTTY_DA_CONFORMANCE_VT220|GHOSTTY_DA_DEVICE_TYPE_VT220|GHOSTTY_DA_FEATURE_(ANSI_COLOR|CLIPBOARD|SELECTIVE_ERASE)|Ghostty(BuildInfo_GHOSTTY_BUILD_INFO_OPTIMIZE|Checkpoint.*|ClipboardLocation_GHOSTTY_CLIPBOARD_LOCATION_(PRIMARY|SELECTION|STANDARD)|ClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_(DENIED|INVALID_DATA|SUCCESS|UNSUPPORTED)|ColorScheme_GHOSTTY_COLOR_SCHEME_(DARK|LIGHT)|FocusEvent_GHOSTTY_FOCUS_(GAINED|LOST)|FormatterFormat_GHOSTTY_FORMATTER_FORMAT_(PLAIN|VT)|KeyAction_GHOSTTY_KEY_ACTION_PRESS|Key_GHOSTTY_KEY_(A|ARROW_DOWN|ARROW_LEFT|ARROW_RIGHT|ARROW_UP|BACKSPACE|END|ENTER|ESCAPE|HOME|TAB)|OptimizeMode_GHOSTTY_OPTIMIZE_RELEASE_FAST|Result_GHOSTTY_(INVALID_VALUE|IO_ERROR|LIMIT_EXCEEDED|NO_VALUE|OUT_OF_MEMORY|OUT_OF_SPACE|SUCCESS)|TerminalCompressionMode_GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL|TerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_(COMPLETE|PENDING|UNSUPPORTED)|TerminalData_GHOSTTY_TERMINAL_DATA_(ACTIVE_SCREEN|COLS|CURSOR_PENDING_WRAP|CURSOR_VISIBLE|CURSOR_X|CURSOR_Y|MODE|PWD|ROWS|TITLE|VT_PROCESSING_ERROR)|TerminalOption_GHOSTTY_TERMINAL_OPT_(BELL|CLIPBOARD_WRITE|COLOR_SCHEME|CONTINUATION_MAX_BYTES|DESKTOP_NOTIFICATION|DEVICE_ATTRIBUTES|GLYPH_PROTOCOL|KITTY_IMAGE_STORAGE_LIMIT|PROGRESS_REPORT|PWD_CHANGED|SCROLLBACK_MAX_BYTES|SCROLLBACK_MAX_LINES|TERMINFO_NAME|TITLE_CHANGED|TITLE_REPORT|USERDATA|WRITE_PTY|SIZE)|TerminalScreen_GHOSTTY_TERMINAL_SCREEN_(ALTERNATE|PRIMARY)))$' \
  -- \
  -I"$include_dir" \
  -target arm64-apple-macos

if grep -Eq '\bGhosttySnapshot|\bghostty_snapshot_' "$tmp_dir/bindings.rs"; then
  printf 'generated bindings exposed prohibited upstream snapshot APIs\n' >&2
  exit 1
fi

mv "$tmp_dir/bindings.rs" "$output"
rustfmt --edition 2024 "$output"
