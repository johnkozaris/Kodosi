#include <ctype.h>
#include <jni.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <ghostty/vt.h>

#include "kodosi_runtime.h"

enum {
    KERNEL_OK = 0,
    KERNEL_NOT_READY = 1,
    KERNEL_OUT_OF_ORDER = 2,
    KERNEL_REJECTED = 3,
};

enum {
    KEY_ENTER,
    KEY_BACKSPACE,
    KEY_TAB,
    KEY_ESCAPE,
    KEY_UP,
    KEY_DOWN,
    KEY_LEFT,
    KEY_RIGHT,
    KEY_HOME,
    KEY_END,
    KEY_PAGE_UP,
    KEY_PAGE_DOWN,
    KEY_DELETE,
    KEY_CHARACTER,
};

enum {
    MOD_SHIFT = 1,
    MOD_CONTROL = 2,
    MOD_ALT = 4,
};

enum {
    CELL_BOLD = 1 << 0,
    CELL_ITALIC = 1 << 1,
    CELL_UNDERLINE = 1 << 2,
    CELL_STRIKETHROUGH = 1 << 3,
    CELL_OVERLINE = 1 << 4,
    CELL_SELECTED = 1 << 5,
    CELL_HYPERLINK = 1 << 6,
};

#define FRAME_HEADER_BYTES 36
#define CELL_BYTES 12
#define MAX_GRAPHEME_BYTES 255
#define MAX_VISIBLE_CELLS 1048576
#define SCROLLBACK_LINES 10000
#define SCROLLBACK_BYTES (46u * 512u * 1024u)

typedef struct {
    pthread_mutex_t lock;
    GhosttyTerminal terminal;
    GhosttyRenderState render;
    GhosttyKeyEncoder keys;
    uint64_t next_sequence;
    bool admitted;
} Kernel;

static const GhosttyColorRgb FOREGROUND = {0xe8, 0xe1, 0xd9};
static const GhosttyColorRgb BACKGROUND = {0x0d, 0x0b, 0x09};
static const GhosttyColorRgb CURSOR = {0xcc, 0x7a, 0x45};
static const GhosttyColorRgb ANSI[16] = {
    {0x1a, 0x16, 0x13}, {0xc4, 0x40, 0x40}, {0x52, 0xb3, 0x6b}, {0xc4, 0x99, 0x3d},
    {0x5d, 0x8e, 0xc7}, {0xb0, 0x6d, 0xba}, {0x5d, 0xaf, 0xa6}, {0xc4, 0xb8, 0xa8},
    {0x8c, 0x7a, 0x6f}, {0xd6, 0x60, 0x60}, {0x6d, 0xcc, 0x85}, {0xd6, 0xb3, 0x5d},
    {0x7d, 0xaa, 0xd9}, {0xc8, 0x8d, 0xd0}, {0x7d, 0xc7, 0xbe}, {0xe8, 0xe1, 0xd9},
};

static Kernel *kernel_of(jlong handle) {
    return (Kernel *)(intptr_t)handle;
}

static uint32_t rgb(GhosttyColorRgb color) {
    return 0xFF000000u | ((uint32_t)color.r << 16) | ((uint32_t)color.g << 8) | color.b;
}

static void put16(uint8_t *at, uint16_t value) {
    at[0] = (uint8_t)value;
    at[1] = (uint8_t)(value >> 8);
}

static void put32(uint8_t *at, uint32_t value) {
    put16(at, (uint16_t)value);
    put16(at + 2, (uint16_t)(value >> 16));
}

static uint32_t clamp32(uint64_t value) {
    return value > UINT32_MAX ? UINT32_MAX : (uint32_t)value;
}

// A snapshot may end before its history, after READY and the image pixels that follow it.
// Once a history page arrived, the snapshot must end with FINISH.
static GhosttyResult restore_history(GhosttySnapshotDecoder decoder, size_t size) {
    bool history = false;
    for (;;) {
        size_t consumed = 0;
        GhosttyResult result =
            ghostty_snapshot_decoder_get(decoder, GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET, &consumed);
        if (result != GHOSTTY_SUCCESS || (!history && consumed == size)) {
            return result;
        }
        result = ghostty_snapshot_decoder_next(decoder);
        if (result == GHOSTTY_NO_VALUE) {
            result = ghostty_snapshot_decoder_get(decoder, GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET, &consumed);
            if (result != GHOSTTY_SUCCESS) {
                return result;
            }
            return consumed == size ? GHOSTTY_SUCCESS : GHOSTTY_INVALID_VALUE;
        }
        if (result != GHOSTTY_SUCCESS) {
            return result;
        }
        GhosttySnapshotDecoderProgressKind kind;
        result = ghostty_snapshot_decoder_get(decoder, GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_KIND, &kind);
        if (result != GHOSTTY_SUCCESS) {
            return result;
        }
        history = history || kind == GHOSTTY_SNAPSHOT_DECODER_PROGRESS_KIND_PAGE;
    }
}

static GhosttyResult configure(GhosttyTerminal terminal, bool console) {
    const uint64_t image_limit = KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES;
    const bool glyphs = false;
    const bool pull_scrollback = !console;
    const bool blink = false;
    const GhosttyTerminalCursorStyle cursor = GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK;
    const size_t scrollback_lines = SCROLLBACK_LINES;
    const size_t scrollback_bytes = SCROLLBACK_BYTES;
    GhosttyResult result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT, &image_limit);
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_GLYPH_PROTOCOL, &glyphs);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK, &pull_scrollback);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE, &cursor);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK, &blink);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, &scrollback_bytes);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &scrollback_lines);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND, &FOREGROUND);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND, &BACKGROUND);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_COLOR_CURSOR, &CURSOR);
    }
    GhosttyColorRgb palette[256];
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_get(terminal, GHOSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT, palette);
    }
    if (result == GHOSTTY_SUCCESS) {
        memcpy(palette, ANSI, sizeof(ANSI));
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_COLOR_PALETTE, palette);
    }
    return result;
}

static void release(Kernel *kernel) {
    ghostty_key_encoder_free(kernel->keys);
    ghostty_render_state_free(kernel->render);
    ghostty_terminal_free(kernel->terminal);
    kernel->keys = NULL;
    kernel->render = NULL;
    kernel->terminal = NULL;
    kernel->admitted = false;
}

static jint install(Kernel *kernel, const uint8_t *snapshot, size_t size, uint64_t next_sequence, int rows, int cols,
                    bool console) {
    if (size == 0 || size > KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES || rows <= 0 || cols <= 0 ||
        (uint64_t)rows * (uint64_t)cols > MAX_VISIBLE_CELLS) {
        return KERNEL_REJECTED;
    }
    GhosttySnapshotDecoder decoder = NULL;
    GhosttyTerminal terminal = NULL;
    GhosttyRenderState render = NULL;
    GhosttyKeyEncoder keys = NULL;
    const bool compress = true;
    const uint64_t image_limit = KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES;
    GhosttyResult result = ghostty_snapshot_decoder_new_buf(NULL, &decoder, snapshot, size);
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_snapshot_decoder_set(decoder, GHOSTTY_SNAPSHOT_DECODER_OPT_COMPRESS_HISTORY, &compress);
    }
    if (result == GHOSTTY_SUCCESS) {
        result =
            ghostty_snapshot_decoder_set(decoder, GHOSTTY_SNAPSHOT_DECODER_OPT_KITTY_IMAGE_STORAGE_LIMIT, &image_limit);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_snapshot_decoder_ready(decoder, &terminal);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = restore_history(decoder, size);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = configure(terminal, console);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_new(NULL, &render);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_update(render, terminal);
    }
    uint16_t state_cols = 0;
    uint16_t state_rows = 0;
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_COLS, &state_cols);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_ROWS, &state_rows);
    }
    if (result == GHOSTTY_SUCCESS && (state_cols != cols || state_rows != rows)) {
        result = GHOSTTY_INVALID_VALUE;
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_key_encoder_new(NULL, &keys);
    }
    ghostty_snapshot_decoder_free(decoder);
    if (result != GHOSTTY_SUCCESS) {
        ghostty_key_encoder_free(keys);
        ghostty_render_state_free(render);
        ghostty_terminal_free(terminal);
        return KERNEL_REJECTED;
    }
    release(kernel);
    kernel->terminal = terminal;
    kernel->render = render;
    kernel->keys = keys;
    kernel->next_sequence = next_sequence;
    kernel->admitted = true;
    return KERNEL_OK;
}

static jint write_bytes(Kernel *kernel, const uint8_t *bytes, size_t length, uint64_t sequence) {
    if (!kernel->admitted) {
        return KERNEL_NOT_READY;
    }
    if (sequence != kernel->next_sequence) {
        return KERNEL_OUT_OF_ORDER;
    }
    ghostty_terminal_vt_write(kernel->terminal, bytes, length);
    bool failed = false;
    if (ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR, &failed) != GHOSTTY_SUCCESS ||
        failed) {
        release(kernel);
        return KERNEL_REJECTED;
    }
    kernel->next_sequence++;
    return KERNEL_OK;
}

typedef struct {
    uint8_t *start;
    size_t capacity;
    size_t used;
} Output;

static uint8_t *reserve(Output *output, size_t bytes) {
    size_t at = output->used;
    output->used += bytes;
    return output->used <= output->capacity ? output->start + at : NULL;
}

static GhosttyResult grapheme(GhosttyRenderStateRowCells cells, Output *output, uint8_t *length) {
    uint8_t text[MAX_GRAPHEME_BYTES + 4];
    GhosttyBuffer buffer = {.ptr = NULL, .cap = 0, .len = 0};
    GhosttyResult result =
        ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8, &buffer);
    if (result == GHOSTTY_SUCCESS && buffer.len == 0) {
        *length = 0;
        return GHOSTTY_SUCCESS;
    }
    if (result != GHOSTTY_OUT_OF_SPACE) {
        return result;
    }
    size_t needed = buffer.len;
    uint8_t *heap = needed > sizeof(text) ? malloc(needed) : NULL;
    buffer.ptr = heap != NULL ? heap : text;
    buffer.cap = heap != NULL ? needed : sizeof(text);
    buffer.len = 0;
    if (needed > buffer.cap) {
        return GHOSTTY_OUT_OF_MEMORY;
    }
    result = ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8, &buffer);
    size_t kept = buffer.len;
    if (kept > MAX_GRAPHEME_BYTES) {
        kept = MAX_GRAPHEME_BYTES;
        while (kept > 0 && (buffer.ptr[kept] & 0xC0) == 0x80) {
            kept--;
        }
    }
    uint8_t *at = result == GHOSTTY_SUCCESS ? reserve(output, kept) : NULL;
    if (at != NULL) {
        memcpy(at, buffer.ptr, kept);
    }
    free(heap);
    *length = (uint8_t)kept;
    return result;
}

// Writes the visible screen: a header, then each cell with its colors, flags, width and text.
static jint frame(Kernel *kernel, Output *output) {
    if (!kernel->admitted) {
        return -KERNEL_NOT_READY;
    }
    GhosttyRenderState render = kernel->render;
    GhosttyResult result = ghostty_render_state_update(render, kernel->terminal);
    uint16_t cols = 0;
    uint16_t rows = 0;
    GhosttyRenderStateColors colors = GHOSTTY_INIT_SIZED(GhosttyRenderStateColors);
    GhosttyRenderStateCursor cursor = GHOSTTY_INIT_SIZED(GhosttyRenderStateCursor);
    GhosttyTerminalScrollbar scrollbar;
    memset(&scrollbar, 0, sizeof(scrollbar));
    bool mouse_tracking = false;
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_COLS, &cols);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_ROWS, &rows);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_COLORS, &colors);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_CURSOR, &cursor);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_SCROLLBAR, &scrollbar);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, &mouse_tracking);
    }
    if (result != GHOSTTY_SUCCESS || (uint64_t)cols * rows > MAX_VISIBLE_CELLS) {
        return -KERNEL_REJECTED;
    }
    uint8_t *header = reserve(output, FRAME_HEADER_BYTES);
    if (header != NULL) {
        bool visible = cursor.visible && cursor.viewport_has_value;
        put16(header, cols);
        put16(header + 2, rows);
        put16(header + 4, cursor.viewport_has_value ? cursor.viewport_x : 0);
        put16(header + 6, cursor.viewport_has_value ? cursor.viewport_y : 0);
        header[8] = visible;
        header[9] = (uint8_t)cursor.visual_style;
        header[10] = mouse_tracking;
        header[11] = cursor.viewport_has_value && cursor.wide_tail;
        put32(header + 12, rgb(colors.foreground));
        put32(header + 16, rgb(colors.background));
        put32(header + 20, rgb(colors.cursor_has_value ? colors.cursor : colors.foreground));
        put32(header + 24, clamp32(scrollbar.total));
        put32(header + 28, clamp32(scrollbar.offset));
        put32(header + 32, clamp32(scrollbar.len));
    }

    GhosttyRenderStateRowIterator row_iterator = NULL;
    GhosttyRenderStateRowCells cells = NULL;
    result = ghostty_render_state_row_iterator_new(NULL, &row_iterator);
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_get(render, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &row_iterator);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_render_state_row_cells_new(NULL, &cells);
    }
    uint16_t row = 0;
    while (result == GHOSTTY_SUCCESS && ghostty_render_state_row_iterator_next(row_iterator)) {
        if (row >= rows) {
            result = GHOSTTY_INVALID_VALUE;
            break;
        }
        GhosttyRenderStateRowSelection selection = GHOSTTY_INIT_SIZED(GhosttyRenderStateRowSelection);
        GhosttyResult selected =
            ghostty_render_state_row_get(row_iterator, GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION, &selection);
        result = ghostty_render_state_row_get(row_iterator, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &cells);
        uint16_t column = 0;
        while (result == GHOSTTY_SUCCESS && ghostty_render_state_row_cells_next(cells)) {
            if (column >= cols) {
                result = GHOSTTY_INVALID_VALUE;
                break;
            }
            GhosttyStyle style = GHOSTTY_INIT_SIZED(GhosttyStyle);
            GhosttyColorRgb foreground = colors.foreground;
            GhosttyColorRgb background = colors.background;
            GhosttyCell raw = 0;
            GhosttyCellWide wide = GHOSTTY_CELL_WIDE_NARROW;
            bool hyperlink = false;
            result = ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &style);
            bool has_foreground =
                ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, &foreground) ==
                GHOSTTY_SUCCESS;
            bool has_background =
                ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, &background) ==
                GHOSTTY_SUCCESS;
            if (result == GHOSTTY_SUCCESS) {
                result = ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, &raw);
            }
            if (result == GHOSTTY_SUCCESS) {
                result = ghostty_cell_get(raw, GHOSTTY_CELL_DATA_WIDE, &wide);
            }
            if (result == GHOSTTY_SUCCESS) {
                result = ghostty_cell_get(raw, GHOSTTY_CELL_DATA_HAS_HYPERLINK, &hyperlink);
            }
            uint8_t *cell = reserve(output, CELL_BYTES);
            uint8_t text_length = 0;
            if (result == GHOSTTY_SUCCESS && !style.invisible) {
                result = grapheme(cells, output, &text_length);
            }
            if (result != GHOSTTY_SUCCESS) {
                break;
            }
            uint16_t index = column++;
            if (cell == NULL) {
                continue;
            }
            if (!has_foreground) {
                foreground = colors.foreground;
            }
            uint32_t fg = rgb(foreground);
            uint32_t bg = has_background ? rgb(background) : 0;
            if (style.inverse) {
                uint32_t swapped = has_background ? bg : rgb(colors.background);
                bg = fg;
                fg = swapped;
            }
            if (style.faint) {
                fg = (fg & 0x00FFFFFFu) | 0xA0000000u;
            }
            bool in_selection = selected == GHOSTTY_SUCCESS && index >= selection.start_x && index <= selection.end_x;
            uint16_t flags = (style.bold ? CELL_BOLD : 0) | (style.italic ? CELL_ITALIC : 0) |
                             (style.underline != 0 ? CELL_UNDERLINE : 0) |
                             (style.strikethrough ? CELL_STRIKETHROUGH : 0) | (style.overline ? CELL_OVERLINE : 0) |
                             (hyperlink ? CELL_HYPERLINK : 0) | (in_selection ? CELL_SELECTED : 0);
            put32(cell, fg);
            put32(cell + 4, bg);
            put16(cell + 8, flags);
            cell[10] = wide == GHOSTTY_CELL_WIDE_WIDE ? 2 : (wide == GHOSTTY_CELL_WIDE_NARROW ? 1 : 0);
            cell[11] = text_length;
        }
        if (result == GHOSTTY_SUCCESS && column != cols) {
            result = GHOSTTY_INVALID_VALUE;
        }
        row++;
    }
    ghostty_render_state_row_cells_free(cells);
    ghostty_render_state_row_iterator_free(row_iterator);
    if (result == GHOSTTY_SUCCESS && row != rows) {
        result = GHOSTTY_INVALID_VALUE;
    }
    if (result != GHOSTTY_SUCCESS) {
        return -KERNEL_REJECTED;
    }
    ghostty_render_state_clean(render);
    return output->used <= output->capacity ? (jint)output->used : -(jint)output->used;
}

static GhosttyKey ghostty_key(jint key, const char *text) {
    switch (key) {
    case KEY_ENTER: return GHOSTTY_KEY_ENTER;
    case KEY_BACKSPACE: return GHOSTTY_KEY_BACKSPACE;
    case KEY_TAB: return GHOSTTY_KEY_TAB;
    case KEY_ESCAPE: return GHOSTTY_KEY_ESCAPE;
    case KEY_UP: return GHOSTTY_KEY_ARROW_UP;
    case KEY_DOWN: return GHOSTTY_KEY_ARROW_DOWN;
    case KEY_LEFT: return GHOSTTY_KEY_ARROW_LEFT;
    case KEY_RIGHT: return GHOSTTY_KEY_ARROW_RIGHT;
    case KEY_HOME: return GHOSTTY_KEY_HOME;
    case KEY_END: return GHOSTTY_KEY_END;
    case KEY_PAGE_UP: return GHOSTTY_KEY_PAGE_UP;
    case KEY_PAGE_DOWN: return GHOSTTY_KEY_PAGE_DOWN;
    case KEY_DELETE: return GHOSTTY_KEY_DELETE;
    default: break;
    }
    char lower = text != NULL && text[0] != '\0' && text[1] == '\0' ? (char)tolower((unsigned char)text[0]) : '\0';
    if (lower >= 'a' && lower <= 'z') {
        return (GhosttyKey)(GHOSTTY_KEY_A + (lower - 'a'));
    }
    if (lower >= '0' && lower <= '9') {
        return (GhosttyKey)(GHOSTTY_KEY_DIGIT_0 + (lower - '0'));
    }
    return GHOSTTY_KEY_UNIDENTIFIED;
}

static jbyteArray encode_key(JNIEnv *env, Kernel *kernel, jint key, jint modifiers, const char *text) {
    if (!kernel->admitted) {
        return NULL;
    }
    GhosttyKeyEvent event = NULL;
    if (ghostty_key_event_new(NULL, &event) != GHOSTTY_SUCCESS) {
        return NULL;
    }
    GhosttyMods mods = (modifiers & MOD_SHIFT ? GHOSTTY_MODS_SHIFT : 0) |
                       (modifiers & MOD_CONTROL ? GHOSTTY_MODS_CTRL : 0) | (modifiers & MOD_ALT ? GHOSTTY_MODS_ALT : 0);
    ghostty_key_encoder_setopt_from_terminal(kernel->keys, kernel->terminal);
    ghostty_key_event_set_action(event, GHOSTTY_KEY_ACTION_PRESS);
    ghostty_key_event_set_key(event, ghostty_key(key, text));
    ghostty_key_event_set_mods(event, mods);
    if (text != NULL && text[0] != '\0') {
        ghostty_key_event_set_utf8(event, text, strlen(text));
        if ((unsigned char)text[0] < 0x80) {
            ghostty_key_event_set_unshifted_codepoint(event, (uint32_t)tolower((unsigned char)text[0]));
        }
    }
    char encoded[128];
    size_t written = 0;
    GhosttyResult result = ghostty_key_encoder_encode(kernel->keys, event, encoded, sizeof(encoded), &written);
    ghostty_key_event_free(event);
    if (result != GHOSTTY_SUCCESS || written > sizeof(encoded)) {
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)written);
    if (array != NULL && written > 0) {
        (*env)->SetByteArrayRegion(env, array, 0, (jsize)written, (const jbyte *)encoded);
    }
    return array;
}

JNIEXPORT jlong JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeCreate(JNIEnv *env, jclass type) {
    (void)env;
    (void)type;
    Kernel *kernel = calloc(1, sizeof(Kernel));
    if (kernel == NULL) {
        return 0;
    }
    pthread_mutex_init(&kernel->lock, NULL);
    return (jlong)(intptr_t)kernel;
}

JNIEXPORT void JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeDestroy(JNIEnv *env, jclass type,
                                                                                     jlong handle) {
    (void)env;
    (void)type;
    Kernel *kernel = kernel_of(handle);
    release(kernel);
    pthread_mutex_destroy(&kernel->lock);
    free(kernel);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeInstall(JNIEnv *env, jclass type,
                                                                                     jlong handle, jobject snapshot,
                                                                                     jlong next_sequence, jint rows,
                                                                                     jint cols, jboolean console) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    const uint8_t *bytes = (*env)->GetDirectBufferAddress(env, snapshot);
    jlong size = (*env)->GetDirectBufferCapacity(env, snapshot);
    if (bytes == NULL || size < 0) {
        return KERNEL_REJECTED;
    }
    pthread_mutex_lock(&kernel->lock);
    jint result = install(kernel, bytes, (size_t)size, (uint64_t)next_sequence, rows, cols, console);
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeWrite(JNIEnv *env, jclass type,
                                                                                   jlong handle, jobject data,
                                                                                   jlong sequence) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    const uint8_t *bytes = (*env)->GetDirectBufferAddress(env, data);
    jlong size = (*env)->GetDirectBufferCapacity(env, data);
    if (bytes == NULL || size < 0) {
        return KERNEL_REJECTED;
    }
    pthread_mutex_lock(&kernel->lock);
    jint result = write_bytes(kernel, bytes, (size_t)size, (uint64_t)sequence);
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeResize(JNIEnv *env, jclass type,
                                                                                    jlong handle, jlong at_sequence,
                                                                                    jint rows, jint cols,
                                                                                    jint cell_width, jint cell_height) {
    (void)env;
    (void)type;
    Kernel *kernel = kernel_of(handle);
    if (rows <= 0 || cols <= 0 || rows > UINT16_MAX || cols > UINT16_MAX || cell_width < 0 || cell_height < 0) {
        return KERNEL_REJECTED;
    }
    pthread_mutex_lock(&kernel->lock);
    jint result = KERNEL_OK;
    if (!kernel->admitted) {
        result = KERNEL_NOT_READY;
    } else if ((uint64_t)at_sequence != kernel->next_sequence) {
        result = KERNEL_OUT_OF_ORDER;
    } else if (ghostty_terminal_resize(kernel->terminal, (uint16_t)cols, (uint16_t)rows, (uint32_t)cell_width,
                                       (uint32_t)cell_height) != GHOSTTY_SUCCESS) {
        result = KERNEL_REJECTED;
    }
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeFrame(JNIEnv *env, jclass type,
                                                                                   jlong handle, jobject buffer) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    Output output = {
        .start = (*env)->GetDirectBufferAddress(env, buffer),
        .capacity = (size_t)(*env)->GetDirectBufferCapacity(env, buffer),
        .used = 0,
    };
    if (output.start == NULL) {
        return -KERNEL_REJECTED;
    }
    pthread_mutex_lock(&kernel->lock);
    jint result = frame(kernel, &output);
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeEncodeKey(JNIEnv *env, jclass type,
                                                                                             jlong handle, jint key,
                                                                                             jint modifiers,
                                                                                             jstring text) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    const char *utf8 = text != NULL ? (*env)->GetStringUTFChars(env, text, NULL) : NULL;
    pthread_mutex_lock(&kernel->lock);
    jbyteArray encoded = encode_key(env, kernel, key, modifiers, utf8);
    pthread_mutex_unlock(&kernel->lock);
    if (utf8 != NULL) {
        (*env)->ReleaseStringUTFChars(env, text, utf8);
    }
    return encoded;
}

JNIEXPORT jboolean JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeScroll(JNIEnv *env, jclass type,
                                                                                        jlong handle, jint rows) {
    (void)env;
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    bool moved = false;
    if (kernel->admitted) {
        GhosttyTerminalScrollbar before;
        GhosttyTerminalScrollbar after;
        memset(&before, 0, sizeof(before));
        memset(&after, 0, sizeof(after));
        ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_SCROLLBAR, &before);
        GhosttyTerminalScrollViewport move;
        memset(&move, 0, sizeof(move));
        move.tag = rows == 0 ? GHOSTTY_SCROLL_VIEWPORT_BOTTOM : GHOSTTY_SCROLL_VIEWPORT_DELTA;
        move.value.delta = rows;
        ghostty_terminal_scroll_viewport(kernel->terminal, move);
        ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_SCROLLBAR, &after);
        moved = before.offset != after.offset;
    }
    pthread_mutex_unlock(&kernel->lock);
    return moved;
}
