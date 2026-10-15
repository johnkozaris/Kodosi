/**
 * @file checkpoint.h
 *
 * Stable, owned JSON semantic terminal checkpoints.
 */
#ifndef GHOSTTY_VT_CHECKPOINT_H
#define GHOSTTY_VT_CHECKPOINT_H

#include <stddef.h>
#include <stdint.h>
#include <ghostty/vt/allocator.h>
#include <ghostty/vt/types.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Current JSON semantic checkpoint schema version. */
#define GHOSTTY_CHECKPOINT_SCHEMA_VERSION 2u

/** Defensive defaults for untrusted checkpoint input and output. */
#define GHOSTTY_CHECKPOINT_DEFAULT_MAX_JSON_BYTES (8u * 1024u * 1024u)
#define GHOSTTY_CHECKPOINT_DEFAULT_MAX_STRING_BYTES (8u * 1024u * 1024u)
#define GHOSTTY_CHECKPOINT_DEFAULT_MAX_CONTINUATION_BYTES (1024u * 1024u)
#define GHOSTTY_CHECKPOINT_DEFAULT_MAX_CELLS (4096u * 4096u)

/** Sized resource limits shared by encode and restore. */
typedef struct {
  size_t size;
  size_t max_json_bytes;
  size_t max_string_bytes;
  size_t max_continuation_bytes;
  size_t max_cells;
} GhosttyCheckpointLimits;

/**
 * Sized checkpoint encoding options. `max_history_rows` is the most rows that
 * are encoded before the visible rows of each screen.
 */
typedef struct {
  size_t size;
  GhosttyCheckpointLimits limits;
  size_t max_history_rows;
} GhosttyCheckpointEncodeOptions;

/** Sized transactional restore options. */
typedef struct {
  size_t size;
  GhosttyCheckpointLimits limits;
} GhosttyCheckpointRestoreOptions;

/**
 * Sized metadata output. NULL is allowed. For a non-NULL output, `size` must
 * describe at least the first size_t. The library writes only fields wholly
 * contained in the declared size and accepts sizes larger than this version.
 * Fields that fit are initialized to deterministic defaults before other
 * argument validation and remain at those defaults unless the operation
 * succeeds. Initialize with GHOSTTY_INIT_SIZED(GhosttyCheckpointInfo).
 */
typedef struct {
  size_t size;
  uint32_t schema_version;
  size_t json_bytes;
  size_t continuation_bytes;
} GhosttyCheckpointInfo;

/** Default initializer for GhosttyCheckpointLimits. */
#define GHOSTTY_CHECKPOINT_LIMITS_INIT \
  { sizeof(GhosttyCheckpointLimits), \
    GHOSTTY_CHECKPOINT_DEFAULT_MAX_JSON_BYTES, \
    GHOSTTY_CHECKPOINT_DEFAULT_MAX_STRING_BYTES, \
    GHOSTTY_CHECKPOINT_DEFAULT_MAX_CONTINUATION_BYTES, \
    GHOSTTY_CHECKPOINT_DEFAULT_MAX_CELLS }

/** Default initializer for GhosttyCheckpointEncodeOptions. */
#define GHOSTTY_CHECKPOINT_ENCODE_OPTIONS_INIT \
  { sizeof(GhosttyCheckpointEncodeOptions), GHOSTTY_CHECKPOINT_LIMITS_INIT, SIZE_MAX }

/** Default initializer for GhosttyCheckpointRestoreOptions. */
#define GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT \
  { sizeof(GhosttyCheckpointRestoreOptions), GHOSTTY_CHECKPOINT_LIMITS_INIT }

/** Return the schema version implemented by this library. */
GHOSTTY_API uint32_t ghostty_checkpoint_schema(void);

/**
 * Encode semantic terminal state and persistent parser continuation as JSON.
 * The complete retained primary/alternate grids, logical page boundaries and
 * capacities, scrollback, styles, hyperlinks, graphemes, cursor state, and
 * parser continuation are encoded. Selections, images, and presentation caches
 * are not encoded. Committed state is constructed directly from semantic JSON;
 * only unfinished parser-continuation bytes are fed to a fresh parser.
 *
 * A NULL/zero buffer is a size query and returns GHOSTTY_OUT_OF_SPACE with the
 * exact required capacity in buffer->len. An undersized non-NULL buffer behaves
 * the same; its written prefix is unspecified and must be discarded.
 *
 * `info` follows the GhosttyCheckpointInfo sized-output contract above. An
 * undersized non-NULL info output returns GHOSTTY_INVALID_VALUE before buffer
 * outputs or terminal state are changed.
 *
 * The caller must serialize this operation with every other access to terminal.
 * Calling this function from a terminal callback on the same terminal is
 * unsupported reentrancy. No callbacks are invoked by encoding.
 */
GHOSTTY_API GhosttyResult ghostty_checkpoint_encode_buf(
    GhosttyTerminal terminal,
    const GhosttyCheckpointEncodeOptions* options,
    GhosttyBuffer* buffer,
    GhosttyCheckpointInfo* info);

/**
 * Encode into an allocator-owned JSON buffer. On success the caller must call
 * ghostty_free(allocator, *out_ptr, *out_len) with exactly the same allocator
 * selection and returned length. Failure always writes NULL and zero outputs,
 * except that an undersized non-NULL `info` is rejected before those outputs.
 * `info` otherwise follows the GhosttyCheckpointInfo contract above.
 */
GHOSTTY_API GhosttyResult ghostty_checkpoint_encode_alloc(
    GhosttyTerminal terminal,
    const GhosttyCheckpointEncodeOptions* options,
    const GhosttyAllocator* allocator,
    uint8_t** out_ptr,
    size_t* out_len,
    GhosttyCheckpointInfo* info);

/**
 * Parse and validate one complete JSON checkpoint, build a complete temporary
 * native terminal and persistent parser stream, then atomically replace the
 * destination semantic state only after every fallible step succeeds.
 *
 * Malformed JSON, unsupported schema versions, limits, and allocation failures
 * leave the destination terminal untouched. Existing callbacks and userdata,
 * terminfo, focus/visibility, APC/DCS/unknown limits, continuation policy, and
 * graphics-disabled policy are destination-owned and preserved. Window-title
 * reporting remains disabled after restore. Existing tracked grid references
 * are detached because checkpoint restore replaces their owning grids.
 * `info` follows the GhosttyCheckpointInfo contract above. An undersized
 * non-NULL output is rejected before the destination is changed.
 *
 * This operation never invokes terminal callbacks. It is not thread-safe with
 * writes, render/search access, encoders configured from the terminal, or any
 * other operation on the same handle. Callers must serialize it. Calling it
 * reentrantly from a callback on the same terminal is unsupported.
 */
GHOSTTY_API GhosttyResult ghostty_checkpoint_restore(
    GhosttyTerminal terminal,
    const uint8_t* json,
    size_t json_len,
    const GhosttyCheckpointRestoreOptions* options,
    GhosttyCheckpointInfo* info);

#ifdef __cplusplus
}
#endif

#endif /* GHOSTTY_VT_CHECKPOINT_H */
