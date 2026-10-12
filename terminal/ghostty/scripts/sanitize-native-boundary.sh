#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_ROOT=${KODOSI_GHOSTTY_VENDOR_ROOT:-"$ROOT/Vendor"}
RENDERER="$VENDOR_ROOT/GhosttyKit.xcframework"
VT="$VENDOR_ROOT/GhosttyVt/macos-arm64"
ARCHIVE="$RENDERER/macos-arm64/libghostty.a"
VT_ARCHIVE="$VT/lib/libghostty-vt.a"

if [[ ! -f "$ARCHIVE" || ! -f "$VT_ARCHIVE" ]]; then
    echo "[!] vendored combined Ghostty archive is missing" >&2
    exit 1
fi
if ! cmp -s "$ARCHIVE" "$VT_ARCHIVE"; then
    echo "[!] renderer and VT paths do not contain the same native image" >&2
    exit 1
fi

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

cat >"$TMP_DIR/vt.c" <<'C'
#define GHOSTTY_STATIC
#include <ghostty/vt.h>

#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

struct callback_state {
    size_t calls;
    size_t bytes;
};

static void write_pty(
    GhosttyTerminal terminal,
    void *userdata,
    const uint8_t *data,
    size_t len
) {
    (void)terminal;
    struct callback_state *state = userdata;
    assert(state != NULL);
    assert(data != NULL || len == 0);
    state->calls += 1;
    state->bytes += len;
}

static void format_screen(
    GhosttyTerminal terminal,
    GhosttyFormatterFormat emit,
    GhosttyTerminalScreen screen
) {
    GhosttyFormatterTerminalOptions options =
        GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions);
    options.emit = emit;
    options.trim = true;
    options.extra.screen.cursor = true;
    options.extra.screen.style = true;
    options.extra.screen.hyperlink = true;
    options.extra.palette = true;
    options.extra.modes = true;
    options.extra.scrolling_region = true;
    options.extra.tabstops = true;
    options.extra.keyboard = true;
    options.screen = &screen;

    GhosttyFormatter formatter = NULL;
    assert(ghostty_formatter_terminal_new(
        NULL,
        &formatter,
        terminal,
        options) == GHOSTTY_SUCCESS);

    size_t required = 0;
    assert(ghostty_formatter_format_buf(
        formatter,
        NULL,
        0,
        &required) == GHOSTTY_OUT_OF_SPACE);
    uint8_t *buffer = malloc(required == 0 ? 1 : required);
    assert(buffer != NULL);
    size_t written = 0;
    assert(ghostty_formatter_format_buf(
        formatter,
        buffer,
        required,
        &written) == GHOSTTY_SUCCESS);
    assert(written == required);
    free(buffer);
    ghostty_formatter_free(formatter);
}

static void checkpoint_info_canary(GhosttyTerminal terminal) {
    GhosttyCheckpointEncodeOptions encode_options =
        GHOSTTY_CHECKPOINT_ENCODE_OPTIONS_INIT;
    GhosttyCheckpointRestoreOptions restore_options =
        GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT;
    struct {
        size_t size;
        uint8_t canary[32];
    } legacy;
    memset(&legacy, 0xA5, sizeof(legacy));
    legacy.size = sizeof(legacy.size);
    uint8_t expected[sizeof(legacy.canary)];
    memcpy(expected, legacy.canary, sizeof(expected));

    GhosttyBuffer query = {0};
    assert(ghostty_checkpoint_schema() == GHOSTTY_CHECKPOINT_SCHEMA_VERSION);
    assert(ghostty_checkpoint_encode_buf(
        terminal,
        &encode_options,
        &query,
        (GhosttyCheckpointInfo *)&legacy) == GHOSTTY_OUT_OF_SPACE);
    assert(query.len > 0);
    assert(memcmp(legacy.canary, expected, sizeof(expected)) == 0);

    uint8_t *json = NULL;
    size_t json_len = 0;
    assert(ghostty_checkpoint_encode_alloc(
        terminal,
        &encode_options,
        NULL,
        &json,
        &json_len,
        (GhosttyCheckpointInfo *)&legacy) == GHOSTTY_SUCCESS);
    assert(json != NULL && json_len == query.len);
    assert(memcmp(legacy.canary, expected, sizeof(expected)) == 0);

    assert(ghostty_checkpoint_restore(
        terminal,
        json,
        json_len,
        &restore_options,
        (GhosttyCheckpointInfo *)&legacy) == GHOSTTY_SUCCESS);
    assert(memcmp(legacy.canary, expected, sizeof(expected)) == 0);
    ghostty_free(NULL, json, json_len);

    legacy.size = 0;
    query.len = 99;
    assert(ghostty_checkpoint_encode_buf(
        terminal,
        &encode_options,
        &query,
        (GhosttyCheckpointInfo *)&legacy) == GHOSTTY_INVALID_VALUE);
    assert(query.len == 99);
    assert(memcmp(legacy.canary, expected, sizeof(expected)) == 0);
}

static void checkpoint_replay_policy_probe(void) {
    static const uint8_t prefix[] = "\x1b_25a1;r;cp=e0a0;AAAA";
    static const uint8_t suffix[] = "AAAAAAAAAA==\x1b\\";
    GhosttyCheckpointEncodeOptions encode_options =
        GHOSTTY_CHECKPOINT_ENCODE_OPTIONS_INIT;
    GhosttyCheckpointRestoreOptions restore_options =
        GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT;

    GhosttyTerminal source = NULL;
    assert(ghostty_terminal_new(NULL, &source, 8, 2) == GHOSTTY_SUCCESS);
    size_t continuation_max = 1024 * 1024;
    assert(ghostty_terminal_set(
        source,
        GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES,
        &continuation_max) == GHOSTTY_SUCCESS);
    ghostty_terminal_vt_write(source, prefix, sizeof(prefix) - 1);
    uint8_t *json = NULL;
    size_t json_len = 0;
    assert(ghostty_checkpoint_encode_alloc(
        source,
        &encode_options,
        NULL,
        &json,
        &json_len,
        NULL) == GHOSTTY_SUCCESS);

    GhosttyTerminal disabled = NULL;
    assert(ghostty_terminal_new(NULL, &disabled, 8, 2) == GHOSTTY_SUCCESS);
    bool glyph_enabled = false;
    assert(ghostty_terminal_set(
        disabled,
        GHOSTTY_TERMINAL_OPT_GLYPH_PROTOCOL,
        &glyph_enabled) == GHOSTTY_SUCCESS);
    assert(ghostty_checkpoint_restore(
        disabled,
        json,
        json_len,
        &restore_options,
        NULL) == GHOSTTY_SUCCESS);
    ghostty_terminal_vt_write(disabled, suffix, sizeof(suffix) - 1);

    uint8_t *disabled_json = NULL;
    size_t disabled_json_len = 0;
    assert(ghostty_checkpoint_encode_alloc(
        disabled,
        &encode_options,
        NULL,
        &disabled_json,
        &disabled_json_len,
        NULL) == GHOSTTY_SUCCESS);
    static const uint8_t needle[] = "\"glyphs\":0";
    assert(memmem(
        disabled_json,
        disabled_json_len,
        needle,
        sizeof(needle) - 1) != NULL);

    ghostty_free(NULL, disabled_json, disabled_json_len);
    ghostty_terminal_free(disabled);
    ghostty_free(NULL, json, json_len);
    ghostty_terminal_free(source);
}

int vt_sanitizer_probe(void) {
    checkpoint_replay_policy_probe();
    static const uint8_t corpus[] =
        "\x1b[31mANSI\x1b[0m e\xcc\x81 "
        "\xf0\x9f\x91\xa8\xe2\x80\x8d\xf0\x9f\x91\xa9\xe2\x80\x8d"
        "\xf0\x9f\x91\xa7\xe2\x80\x8d\xf0\x9f\x91\xa6\r\n"
        "\x1b]8;;https://example.test\x1b\\link\x1b]8;;;;\x1b\\\r\n"
        "\x1b[?1049halternate\x1b[?1049l"
        "\x1b[6n\x1bP$qm\x1b\\"
        "\x1b]52;c;Y2xpcGJvYXJk\x07"
        "\x1b_Gf=100,a=T;AAAA\x1b\\";
    static const uint8_t unfinished[] = "\x1b[38;2;1;2";
    static const uint8_t finish[] = ";3mcontinued\x1b[0m\r\n";
    static const uint8_t malformed[] = {
        0xff, 0xfe, 0x1b, ']', '9', ';', 'x', 0x1b, '\\', 0x00, 0x9b,
    };

    for (size_t iteration = 0; iteration < 256; iteration++) {
        GhosttyTerminal terminal = NULL;
        assert(ghostty_terminal_new(NULL, &terminal, 80, 24) == GHOSTTY_SUCCESS);
        if (iteration == 0) checkpoint_info_canary(terminal);

        struct callback_state state = {0};
        assert(ghostty_terminal_set(
            terminal,
            GHOSTTY_TERMINAL_OPT_USERDATA,
            &state) == GHOSTTY_SUCCESS);
        assert(ghostty_terminal_set(
            terminal,
            GHOSTTY_TERMINAL_OPT_WRITE_PTY,
            (const void *)write_pty) == GHOSTTY_SUCCESS);

        size_t max_bytes = 64 * 1024 * 1024;
        size_t max_lines = 1024;
        size_t continuation_max = 1024 * 1024;
        assert(ghostty_terminal_set(
            terminal,
            GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,
            &max_bytes) == GHOSTTY_SUCCESS);
        assert(ghostty_terminal_set(
            terminal,
            GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES,
            &max_lines) == GHOSTTY_SUCCESS);
        assert(ghostty_terminal_set(
            terminal,
            GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES,
            &continuation_max) == GHOSTTY_SUCCESS);

        for (size_t split = 0; split <= sizeof(corpus) - 1; split++) {
            ghostty_terminal_vt_write(terminal, corpus, split);
            ghostty_terminal_vt_write(
                terminal,
                corpus + split,
                sizeof(corpus) - 1 - split);
        }
        ghostty_terminal_vt_write(terminal, malformed, sizeof(malformed));
        ghostty_terminal_vt_write(terminal, unfinished, sizeof(unfinished) - 1);

        uint8_t *continuation = NULL;
        size_t continuation_len = 0;
        assert(ghostty_terminal_continuation_alloc(
            terminal,
            NULL,
            &continuation,
            &continuation_len) == GHOSTTY_SUCCESS);
        assert(continuation_len == sizeof(unfinished) - 1);
        assert(memcmp(continuation, unfinished, continuation_len) == 0);
        ghostty_free(NULL, continuation, continuation_len);

        ghostty_terminal_vt_write(terminal, finish, sizeof(finish) - 1);
        for (size_t line = 0; line < 4096; line++) {
            ghostty_terminal_vt_write(
                terminal,
                (const uint8_t *)"compressible history line\r\n",
                sizeof("compressible history line\r\n") - 1);
        }

        assert(ghostty_terminal_resize(terminal, 200, 50, 8, 16) == GHOSTTY_SUCCESS);
        assert(ghostty_terminal_resize(terminal, 80, 24, 8, 16) == GHOSTTY_SUCCESS);
        format_screen(terminal, GHOSTTY_FORMATTER_FORMAT_PLAIN, GHOSTTY_TERMINAL_SCREEN_PRIMARY);
        format_screen(terminal, GHOSTTY_FORMATTER_FORMAT_VT, GHOSTTY_TERMINAL_SCREEN_PRIMARY);
        format_screen(terminal, GHOSTTY_FORMATTER_FORMAT_VT, GHOSTTY_TERMINAL_SCREEN_ALTERNATE);

        GhosttyTerminalCompressionResult compression;
        do {
            assert(ghostty_terminal_compress(
                terminal,
                GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL,
                &compression) == GHOSTTY_SUCCESS);
        } while (compression == GHOSTTY_TERMINAL_COMPRESSION_RESULT_PENDING);

        assert(state.calls > 0);
        assert(state.bytes > 0);
        ghostty_terminal_free(terminal);
    }

    return 0;
}
C

cat >"$TMP_DIR/renderer.c" <<'C'
#include "ghostty.h"

#include <assert.h>
#include <stdint.h>
#include <string.h>

static void renderer_checkpoint_symbols(void) {
    ghostty_result_e (*restore_checkpoint)(
        ghostty_surface_t,
        const uint8_t *,
        size_t,
        const ghostty_checkpoint_restore_options_s *,
        ghostty_checkpoint_info_s *) = ghostty_surface_checkpoint_restore;
    assert(restore_checkpoint != NULL);

    struct {
        size_t size;
        uint8_t canary[32];
    } legacy;
    memset(&legacy, 0xA5, sizeof(legacy));
    legacy.size = sizeof(legacy.size);
    uint8_t expected[sizeof(legacy.canary)];
    memcpy(expected, legacy.canary, sizeof(expected));
    ghostty_checkpoint_restore_options_s options =
        GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT;
    assert(restore_checkpoint(
        NULL,
        NULL,
        0,
        &options,
        (ghostty_checkpoint_info_s *)&legacy) == GHOSTTY_RESULT_INVALID_VALUE);
    assert(memcmp(legacy.canary, expected, sizeof(expected)) == 0);
}

int renderer_sanitizer_probe(int argc, char **argv) {
    renderer_checkpoint_symbols();
    ghostty_info_s info = ghostty_info();
    assert(info.build_mode == GHOSTTY_BUILD_MODE_RELEASE_FAST);
    return ghostty_init((uintptr_t)argc, argv);
}
C

cat >"$TMP_DIR/main.c" <<'C'
int renderer_sanitizer_probe(int argc, char **argv);
int vt_sanitizer_probe(void);

int main(int argc, char **argv) {
    int result = renderer_sanitizer_probe(argc, argv);
    return result == 0 ? vt_sanitizer_probe() : result;
}
C

COMMON_FLAGS=(
    -arch arm64
    -mmacosx-version-min=13.0
    -O1
    -g
    -fno-omit-frame-pointer
    '-fsanitize=address,undefined'
    -fno-sanitize-recover=all
)

xcrun clang "${COMMON_FLAGS[@]}" \
    -I"$VT/include" \
    -c "$TMP_DIR/vt.c" \
    -o "$TMP_DIR/vt.o"
xcrun clang "${COMMON_FLAGS[@]}" \
    -I"$RENDERER/macos-arm64/Headers" \
    -c "$TMP_DIR/renderer.c" \
    -o "$TMP_DIR/renderer.o"
xcrun clang "${COMMON_FLAGS[@]}" \
    -c "$TMP_DIR/main.c" \
    -o "$TMP_DIR/main.o"
xcrun clang "${COMMON_FLAGS[@]}" \
    "$TMP_DIR/vt.o" \
    "$TMP_DIR/renderer.o" \
    "$TMP_DIR/main.o" \
    "$ARCHIVE" \
    -lc++ \
    -framework Foundation \
    -framework CoreFoundation \
    -framework CoreGraphics \
    -framework CoreText \
    -framework CoreVideo \
    -framework QuartzCore \
    -framework IOSurface \
    -framework Carbon \
    -framework Metal \
    -o "$TMP_DIR/native-boundary-sanitizer"

ASAN_OPTIONS="detect_leaks=0:halt_on_error=1:strict_string_checks=1" \
UBSAN_OPTIONS="halt_on_error=1:print_stacktrace=1" \
    "$TMP_DIR/native-boundary-sanitizer"

echo "[*] ASan/UBSan C ABI checks passed against the exact combined shipping image"
echo "[*] scope: the C harness and sanitizer runtime are instrumented; prebuilt Zig objects are exercised but not compiler-instrumented"
