#include <ctype.h>
#include <android/bitmap.h>
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
    uint8_t *clipboard;
    size_t clipboard_len;
    uint8_t clipboard_kind;
    bool clipboard_read;
} Kernel;

static JavaVM *java_vm;
static jclass kernel_class;
static jmethodID decode_png_method;
static pthread_mutex_t platform_lock = PTHREAD_MUTEX_INITIALIZER;

static bool decode_png(void *userdata, const GhosttyAllocator *allocator,
                       const uint8_t *data, size_t length, GhosttySysImage *out) {
    (void)userdata;
    JNIEnv *env = NULL;
    if (java_vm == NULL || length > KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES ||
        (*java_vm)->GetEnv(java_vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return false;
    }
    jbyteArray encoded = (*env)->NewByteArray(env, (jsize)length);
    if (encoded == NULL) return false;
    (*env)->SetByteArrayRegion(env, encoded, 0, (jsize)length, (const jbyte *)data);
    jobject bitmap = (*env)->CallStaticObjectMethod(env, kernel_class, decode_png_method, encoded);
    (*env)->DeleteLocalRef(env, encoded);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        return false;
    }
    if (bitmap == NULL) return false;
    AndroidBitmapInfo info;
    void *pixels = NULL;
    bool success = false;
    if (AndroidBitmap_getInfo(env, bitmap, &info) == ANDROID_BITMAP_RESULT_SUCCESS &&
        info.format == ANDROID_BITMAP_FORMAT_RGBA_8888 &&
        (uint64_t)info.width * info.height * 4 <= KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES &&
        AndroidBitmap_lockPixels(env, bitmap, &pixels) == ANDROID_BITMAP_RESULT_SUCCESS) {
        size_t bytes = (size_t)info.width * info.height * 4;
        uint8_t *copy = ghostty_alloc(allocator, bytes);
        if (copy != NULL) {
            for (uint32_t y = 0; y < info.height; y++) {
                memcpy(copy + (size_t)y * info.width * 4, (uint8_t *)pixels + (size_t)y * info.stride, (size_t)info.width * 4);
            }
            *out = (GhosttySysImage){.width = info.width, .height = info.height, .data = copy, .data_len = bytes};
            success = true;
        }
        AndroidBitmap_unlockPixels(env, bitmap);
    }
    (*env)->DeleteLocalRef(env, bitmap);
    return success;
}

static bool mime_is(GhosttyString mime, const char *name) {
    return mime.len == strlen(name) && memcmp(mime.ptr, name, mime.len) == 0;
}

static void clipboard_write(GhosttyTerminal terminal, void *userdata, const GhosttyClipboardWrite *write) {
    (void)terminal;
    Kernel *kernel = userdata;
    if (write == NULL || write->size < sizeof(*write) || write->reply == NULL) return;
    uint8_t kind = write->contents_len == 0 ? 4 : 0;
    const GhosttyString *data = NULL;
    for (size_t i = 0; i < write->contents_len; i++) {
        const GhosttyClipboardContent *part = &write->contents[i];
        if (mime_is(part->mime, "text/plain") || mime_is(part->mime, "text/plain;charset=utf-8") ||
            mime_is(part->mime, "UTF8_STRING") || mime_is(part->mime, "TEXT") || mime_is(part->mime, "STRING")) {
            kind = 1;
            data = &part->data;
            break;
        }
        if (mime_is(part->mime, "image/png")) {
            kind = 2;
            data = &part->data;
        } else if (mime_is(part->mime, "text/html") && kind == 0) {
            kind = 3;
            data = &part->data;
        }
    }
    GhosttyClipboardWriteReply reply = {.size = sizeof(reply),
        .result = GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED};
    size_t length = data != NULL ? data->len : 0;
    if (kind != 0 && length <= KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES) {
        uint8_t *copy = malloc(length + 1);
        if (copy != NULL) {
            if (length != 0) memcpy(copy, data->ptr, length);
            free(kernel->clipboard);
            kernel->clipboard = copy;
            kernel->clipboard_len = length;
            kernel->clipboard_kind = kind;
            reply.result = GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS;
        } else {
            reply.result = GHOSTTY_CLIPBOARD_WRITE_RESULT_IO_ERROR;
        }
    }
    write->reply(write, &reply);
}

static void clipboard_read(GhosttyTerminal terminal, void *userdata, const GhosttyClipboardRead *read) {
    (void)terminal;
    Kernel *kernel = userdata;
    if (read != NULL && read->size >= sizeof(*read) && read->protocol == GHOSTTY_CLIPBOARD_PROTOCOL_OSC_52) {
        kernel->clipboard_read = true;
    }
}

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

static GhosttyResult configure(GhosttyTerminal terminal, bool console, Kernel *kernel) {
    const uint64_t image_limit = KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES;
    const bool glyphs = false;
    const bool pull_scrollback = !console;
    const bool blink = false;
    const GhosttyTerminalCursorStyle cursor = GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK;
    const size_t scrollback_lines = SCROLLBACK_LINES;
    const size_t scrollback_bytes = SCROLLBACK_BYTES;
    GhosttyResult result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_USERDATA, kernel);
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE, (const void *)clipboard_write);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_CLIPBOARD_READ, (const void *)clipboard_read);
    }
    if (result == GHOSTTY_SUCCESS) {
        result = ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT, &image_limit);
    }
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
    free(kernel->clipboard);
    kernel->clipboard = NULL;
    kernel->clipboard_len = 0;
    kernel->clipboard_kind = 0;
    kernel->clipboard_read = false;
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
        result = configure(terminal, console, kernel);
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
        GhosttyRow raw_row = 0;
        bool wrapped = false;
        bool continuation = false;
        result = ghostty_render_state_row_get(row_iterator, GHOSTTY_RENDER_STATE_ROW_DATA_RAW, &raw_row);
        if (result == GHOSTTY_SUCCESS) {
            result = ghostty_row_get(raw_row, GHOSTTY_ROW_DATA_WRAP, &wrapped);
        }
        if (result == GHOSTTY_SUCCESS) {
            result = ghostty_row_get(raw_row, GHOSTTY_ROW_DATA_WRAP_CONTINUATION, &continuation);
        }
        uint8_t *row_flags = reserve(output, 1);
        if (row_flags != NULL) {
            *row_flags = (wrapped ? 1 : 0) | (continuation ? 2 : 0);
        }
        if (result == GHOSTTY_SUCCESS) {
            result = ghostty_render_state_row_get(row_iterator, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &cells);
        }
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

static jbyteArray encode_paste(JNIEnv *env, Kernel *kernel, char *text, size_t length) {
    if (!kernel->admitted || length == 0) {
        return NULL;
    }
    GhosttyTerminalModeConfig bracketed = {.mode = GHOSTTY_MODE_BRACKETED_PASTE, .value = false};
    if (ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_MODE, &bracketed) != GHOSTTY_SUCCESS) {
        return NULL;
    }
    size_t needed = 0;
    GhosttyResult result = ghostty_paste_encode(text, length, bracketed.value, NULL, 0, &needed);
    if (result != GHOSTTY_OUT_OF_SPACE || needed > KODOSI_MAX_FRAME_BYTES) {
        return NULL;
    }
    char *encoded = malloc(needed);
    size_t written = 0;
    if (encoded == NULL ||
        ghostty_paste_encode(text, length, bracketed.value, encoded, needed, &written) != GHOSTTY_SUCCESS) {
        free(encoded);
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)written);
    if (array != NULL) {
        (*env)->SetByteArrayRegion(env, array, 0, (jsize)written, (const jbyte *)encoded);
    }
    free(encoded);
    return array;
}

JNIEXPORT jlong JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeCreate(JNIEnv *env, jclass type) {
    pthread_mutex_lock(&platform_lock);
    if (kernel_class == NULL) {
        (*env)->GetJavaVM(env, &java_vm);
        kernel_class = (*env)->NewGlobalRef(env, type);
        decode_png_method = (*env)->GetStaticMethodID(env, type, "decodePng", "([B)Landroid/graphics/Bitmap;");
        ghostty_sys_set(GHOSTTY_SYS_OPT_DECODE_PNG, (const void *)decode_png);
    }
    pthread_mutex_unlock(&platform_lock);
    if (kernel_class == NULL || decode_png_method == NULL) return 0;
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

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeEncodePaste(JNIEnv *env, jclass type,
                                                                                               jlong handle,
                                                                                               jbyteArray text) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    jsize length = (*env)->GetArrayLength(env, text);
    char *copy = malloc((size_t)length + 1);
    if (copy == NULL) {
        return NULL;
    }
    (*env)->GetByteArrayRegion(env, text, 0, length, (jbyte *)copy);
    pthread_mutex_lock(&kernel->lock);
    jbyteArray encoded = encode_paste(env, kernel, copy, (size_t)length);
    pthread_mutex_unlock(&kernel->lock);
    free(copy);
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

static jbyteArray byte_array(JNIEnv *env, const uint8_t *bytes, size_t length) {
    if (length > KODOSI_MAX_FRAME_BYTES) {
        return NULL;
    }
    jbyteArray result = (*env)->NewByteArray(env, (jsize)length);
    if (result != NULL && length > 0) {
        (*env)->SetByteArrayRegion(env, result, 0, (jsize)length, (const jbyte *)bytes);
    }
    return result;
}

static GhosttyResult viewport_ref(Kernel *kernel, jint column, jint row, GhosttyGridRef *ref) {
    if (!kernel->admitted || column < 0 || column > UINT16_MAX || row < 0) {
        return GHOSTTY_INVALID_VALUE;
    }
    GhosttyPoint point = {.tag = GHOSTTY_POINT_TAG_VIEWPORT};
    point.value.coordinate.x = (uint16_t)column;
    point.value.coordinate.y = (uint32_t)row;
    return ghostty_terminal_grid_ref(kernel->terminal, point, ref);
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeLink(
    JNIEnv *env, jclass type, jlong handle, jint column, jint row) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyGridRef ref = {.size = sizeof(GhosttyGridRef)};
    jbyteArray result = NULL;
    size_t length = 0;
    if (viewport_ref(kernel, column, row, &ref) == GHOSTTY_SUCCESS &&
        ghostty_grid_ref_hyperlink_uri(&ref, NULL, 0, &length) == GHOSTTY_OUT_OF_SPACE &&
        length > 0 && length <= KODOSI_MAX_FRAME_BYTES) {
        uint8_t *bytes = malloc(length);
        if (bytes != NULL && ghostty_grid_ref_hyperlink_uri(&ref, bytes, length, &length) == GHOSTTY_SUCCESS) {
            result = byte_array(env, bytes, length);
        }
        free(bytes);
    }
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT void JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeSelect(
    JNIEnv *env, jclass type, jlong handle, jint column, jint row, jboolean word) {
    (void)env;
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    if (kernel->admitted) {
        GhosttySelection selection = {.size = sizeof(GhosttySelection)};
        GhosttyResult result = GHOSTTY_NO_VALUE;
        if (column == -2) {
            ghostty_terminal_set(kernel->terminal, GHOSTTY_TERMINAL_OPT_SELECTION, NULL);
        } else if (column == -1) {
            result = ghostty_terminal_select_all(kernel->terminal, &selection);
        } else {
            GhosttyGridRef ref = {.size = sizeof(GhosttyGridRef)};
            if (viewport_ref(kernel, column, row, &ref) == GHOSTTY_SUCCESS) {
                if (word) {
                    GhosttyTerminalSelectWordOptions options = {.size = sizeof(options), .ref = ref};
                    result = ghostty_terminal_select_word(kernel->terminal, &options, &selection);
                } else {
                    result = ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_SELECTION, &selection);
                    if (result != GHOSTTY_SUCCESS) {
                        selection.start = ref;
                    }
                    selection.end = ref;
                    result = GHOSTTY_SUCCESS;
                }
            }
        }
        if (result == GHOSTTY_SUCCESS) {
            ghostty_terminal_set(kernel->terminal, GHOSTTY_TERMINAL_OPT_SELECTION, &selection);
        }
    }
    pthread_mutex_unlock(&kernel->lock);
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeSelected(
    JNIEnv *env, jclass type, jlong handle) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyTerminalSelectionFormatOptions options = {
        .size = sizeof(options), .emit = GHOSTTY_FORMATTER_FORMAT_PLAIN, .unwrap = true, .trim = true};
    size_t length = 0;
    jbyteArray result = NULL;
    if (kernel->admitted &&
        ghostty_terminal_selection_format_buf(kernel->terminal, options, NULL, 0, &length) == GHOSTTY_OUT_OF_SPACE &&
        length <= KODOSI_MAX_FRAME_BYTES) {
        uint8_t *bytes = malloc(length);
        if (bytes != NULL &&
            ghostty_terminal_selection_format_buf(kernel->terminal, options, bytes, length, &length) == GHOSTTY_SUCCESS) {
            result = byte_array(env, bytes, length);
        }
        free(bytes);
    }
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeMouse(
    JNIEnv *env, jclass type, jlong handle, jint action, jfloat x, jfloat y, jint width, jint height,
    jint cell_width, jint cell_height, jint button, jint modifiers) {
    (void)type;
    if (action < 0 || action > 2 || width <= 0 || height <= 0 || cell_width <= 0 || cell_height <= 0 ||
        button < 0 || button > 11) {
        return NULL;
    }
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyMouseEvent event = NULL;
    GhosttyMouseEncoder encoder = NULL;
    jbyteArray result = NULL;
    if (kernel->admitted && ghostty_mouse_event_new(NULL, &event) == GHOSTTY_SUCCESS &&
        ghostty_mouse_encoder_new(NULL, &encoder) == GHOSTTY_SUCCESS) {
        ghostty_mouse_encoder_setopt_from_terminal(encoder, kernel->terminal);
        GhosttyMouseEncoderSize size = {.size = sizeof(size), .screen_width = (uint32_t)width,
            .screen_height = (uint32_t)height, .cell_width = (uint32_t)cell_width, .cell_height = (uint32_t)cell_height};
        const bool pressed = action != GHOSTTY_MOUSE_ACTION_RELEASE && button != 0;
        ghostty_mouse_encoder_setopt(encoder, GHOSTTY_MOUSE_ENCODER_OPT_SIZE, &size);
        ghostty_mouse_encoder_setopt(encoder, GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED, &pressed);
        ghostty_mouse_event_set_action(event, (GhosttyMouseAction)action);
        if (button != 0) {
            ghostty_mouse_event_set_button(event, (GhosttyMouseButton)button);
        }
        ghostty_mouse_event_set_position(event, (GhosttyMousePosition){.x = x, .y = y});
        ghostty_mouse_event_set_mods(event, (modifiers & MOD_SHIFT ? GHOSTTY_MODS_SHIFT : 0) |
            (modifiers & MOD_CONTROL ? GHOSTTY_MODS_CTRL : 0) | (modifiers & MOD_ALT ? GHOSTTY_MODS_ALT : 0));
        char bytes[128];
        size_t length = 0;
        if (ghostty_mouse_encoder_encode(encoder, event, bytes, sizeof(bytes), &length) == GHOSTTY_SUCCESS) {
            result = byte_array(env, (const uint8_t *)bytes, length);
        }
    }
    ghostty_mouse_event_free(event);
    ghostty_mouse_encoder_free(encoder);
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeClipboard(
    JNIEnv *env, jclass type, jlong handle) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    jbyteArray result = NULL;
    if (kernel->clipboard_kind != 0 || kernel->clipboard_read) {
        result = (*env)->NewByteArray(env, (jsize)kernel->clipboard_len + 1);
        if (result != NULL) {
            jbyte flags = (jbyte)(kernel->clipboard_kind | (kernel->clipboard_read ? 8 : 0));
            (*env)->SetByteArrayRegion(env, result, 0, 1, &flags);
            if (kernel->clipboard_len != 0) {
                (*env)->SetByteArrayRegion(env, result, 1, (jsize)kernel->clipboard_len, (const jbyte *)kernel->clipboard);
            }
        }
        free(kernel->clipboard);
        kernel->clipboard = NULL;
        kernel->clipboard_len = 0;
        kernel->clipboard_kind = 0;
        kernel->clipboard_read = false;
    }
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jlong JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeTick(
    JNIEnv *env, jclass type, jlong handle, jlong now) {
    (void)env;
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyKittyGraphics graphics = NULL;
    uint64_t next = 0;
    jlong result = -1;
    if (kernel->admitted &&
        ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS, &graphics) == GHOSTTY_SUCCESS &&
        ghostty_kitty_graphics_animation_tick(graphics, (uint64_t)now, &next) == GHOSTTY_SUCCESS) {
        result = next > (uint64_t)now ? (jlong)(next - (uint64_t)now) : 0;
    }
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jlongArray JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeImages(
    JNIEnv *env, jclass type, jlong handle) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyRenderStateImageIterator iterator = NULL;
    GhosttyKittyGraphics graphics = NULL;
    jlong *placements = NULL;
    size_t count = 0;
    size_t capacity = 0;
    if (kernel->admitted &&
        ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS, &graphics) == GHOSTTY_SUCCESS &&
        ghostty_render_state_image_iterator_new(NULL, &iterator) == GHOSTTY_SUCCESS &&
        ghostty_render_state_get(kernel->render, GHOSTTY_RENDER_STATE_DATA_IMAGE_ITERATOR, &iterator) == GHOSTTY_SUCCESS) {
        while (ghostty_render_state_image_iterator_next(iterator) && count < MAX_VISIBLE_CELLS) {
            GhosttyImageProtocol protocol;
            GhosttyRenderStateImageLayer layer;
            uint32_t id, ox, oy, width, height, sx, sy, sw, sh, iw = 0, ih = 0;
            uint64_t generation;
            int32_t column, row, z;
            const GhosttyRenderStateImageData keys[] = {
                GHOSTTY_RENDER_STATE_IMAGE_DATA_PROTOCOL, GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_ID,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_GENERATION, GHOSTTY_RENDER_STATE_IMAGE_DATA_LAYER,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_Z, GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_COL,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_ROW, GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_X,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_Y, GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_WIDTH,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_HEIGHT, GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_X,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_Y, GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_WIDTH,
                GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_HEIGHT,
            };
            void *values[] = {&protocol, &id, &generation, &layer, &z, &column, &row, &ox, &oy,
                &width, &height, &sx, &sy, &sw, &sh};
            if (ghostty_render_state_image_get_multi(iterator, 15, keys, values, NULL) != GHOSTTY_SUCCESS ||
                protocol != GHOSTTY_IMAGE_PROTOCOL_KITTY) continue;
            GhosttyKittyGraphicsImage image = ghostty_kitty_graphics_image(graphics, id);
            if (ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_WIDTH, &iw) != GHOSTTY_SUCCESS ||
                ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_HEIGHT, &ih) != GHOSTTY_SUCCESS) continue;
            if (count == capacity) {
                size_t next = capacity == 0 ? 16 : capacity * 2;
                jlong *grown = realloc(placements, next * 16 * sizeof(jlong));
                if (grown == NULL) break;
                placements = grown;
                capacity = next;
            }
            const jlong part[] = {(jlong)id, (jlong)generation, (jlong)layer, z, column, row, ox, oy,
                width, height, sx, sy, sw, sh, iw, ih};
            memcpy(placements + count * 16, part, sizeof(part));
            count++;
        }
    }
    jlongArray result = (*env)->NewLongArray(env, (jsize)(count * 16));
    if (result != NULL && count != 0) {
        (*env)->SetLongArrayRegion(env, result, 0, (jsize)(count * 16), placements);
    }
    free(placements);
    ghostty_render_state_image_iterator_free(iterator);
    pthread_mutex_unlock(&kernel->lock);
    return result;
}

JNIEXPORT jobject JNICALL Java_com_kodosi_mobile_terminals_TerminalKernel_nativeImage(
    JNIEnv *env, jclass type, jlong handle, jlong id, jlong expected_generation, jlong maximum_pixels) {
    (void)type;
    Kernel *kernel = kernel_of(handle);
    pthread_mutex_lock(&kernel->lock);
    GhosttyKittyGraphics graphics = NULL;
    GhosttyKittyGraphicsImage image = NULL;
    uint32_t width = 0, height = 0;
    uint64_t generation = 0;
    const uint8_t *pixels = NULL;
    size_t length = 0;
    GhosttyKittyImageFormat format;
    jobject bitmap = NULL;
    if (!kernel->admitted ||
        ghostty_terminal_get(kernel->terminal, GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS, &graphics) != GHOSTTY_SUCCESS ||
        (image = ghostty_kitty_graphics_image(graphics, (uint32_t)id)) == NULL ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_GENERATION, &generation) != GHOSTTY_SUCCESS ||
        generation != (uint64_t)expected_generation ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_WIDTH, &width) != GHOSTTY_SUCCESS ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_HEIGHT, &height) != GHOSTTY_SUCCESS ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_DATA_PTR, &pixels) != GHOSTTY_SUCCESS ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_DATA_LEN, &length) != GHOSTTY_SUCCESS ||
        ghostty_kitty_graphics_image_get(image, GHOSTTY_KITTY_IMAGE_DATA_FORMAT, &format) != GHOSTTY_SUCCESS ||
        width == 0 || height == 0 || pixels == NULL || maximum_pixels < 1 ||
        length > KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES) goto finish;
    unsigned bpp;
    switch (format) {
        case GHOSTTY_KITTY_IMAGE_FORMAT_RGB: bpp = 3; break;
        case GHOSTTY_KITTY_IMAGE_FORMAT_RGBA: bpp = 4; break;
        case GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA: bpp = 2; break;
        case GHOSTTY_KITTY_IMAGE_FORMAT_GRAY: bpp = 1; break;
        default: goto finish;
    }
    if ((uint64_t)width * height * bpp != length) goto finish;
    uint32_t sample = 1;
    while ((width + sample - 1) / sample > 8192 || (height + sample - 1) / sample > 8192 ||
        (uint64_t)((width + sample - 1) / sample) * ((height + sample - 1) / sample) >
            (uint64_t)maximum_pixels) sample *= 2;
    uint32_t output_width = (width + sample - 1) / sample;
    uint32_t output_height = (height + sample - 1) / sample;
    jclass bitmap_type = (*env)->FindClass(env, "android/graphics/Bitmap");
    jclass config_type = (*env)->FindClass(env, "android/graphics/Bitmap$Config");
    if (bitmap_type == NULL || config_type == NULL) goto finish;
    jfieldID config_field = (*env)->GetStaticFieldID(env, config_type, "ARGB_8888", "Landroid/graphics/Bitmap$Config;");
    jmethodID create = (*env)->GetStaticMethodID(env, bitmap_type, "createBitmap", "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;");
    jobject config = config_field != NULL ? (*env)->GetStaticObjectField(env, config_type, config_field) : NULL;
    if (create != NULL && config != NULL) {
        bitmap = (*env)->CallStaticObjectMethod(env, bitmap_type, create, (jint)output_width, (jint)output_height, config);
    }
    (*env)->DeleteLocalRef(env, bitmap_type);
    (*env)->DeleteLocalRef(env, config_type);
    if (config != NULL) (*env)->DeleteLocalRef(env, config);
    if ((*env)->ExceptionCheck(env) || bitmap == NULL) goto finish;
    AndroidBitmapInfo info;
    void *destination = NULL;
    if (AndroidBitmap_getInfo(env, bitmap, &info) != ANDROID_BITMAP_RESULT_SUCCESS ||
        info.format != ANDROID_BITMAP_FORMAT_RGBA_8888 ||
        AndroidBitmap_lockPixels(env, bitmap, &destination) != ANDROID_BITMAP_RESULT_SUCCESS) {
        (*env)->DeleteLocalRef(env, bitmap);
        bitmap = NULL;
        goto finish;
    }
    for (uint32_t y = 0; y < output_height; y++) {
        uint8_t *line = (uint8_t *)destination + (size_t)y * info.stride;
        for (uint32_t x = 0; x < output_width; x++) {
            const uint8_t *pixel = pixels + ((size_t)y * sample * width + (size_t)x * sample) * bpp;
            uint8_t alpha = format == GHOSTTY_KITTY_IMAGE_FORMAT_RGBA ? pixel[3] :
                (format == GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA ? pixel[1] : 255);
            const bool gray = format == GHOSTTY_KITTY_IMAGE_FORMAT_GRAY || format == GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA;
            line[x * 4] = (uint8_t)((unsigned)pixel[0] * alpha / 255);
            line[x * 4 + 1] = (uint8_t)((unsigned)pixel[gray ? 0 : 1] * alpha / 255);
            line[x * 4 + 2] = (uint8_t)((unsigned)pixel[gray ? 0 : 2] * alpha / 255);
            line[x * 4 + 3] = alpha;
        }
    }
    AndroidBitmap_unlockPixels(env, bitmap);
finish:
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        bitmap = NULL;
    }
    pthread_mutex_unlock(&kernel->lock);
    return bitmap;
}
