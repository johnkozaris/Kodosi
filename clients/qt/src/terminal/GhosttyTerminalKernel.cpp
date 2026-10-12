#include "terminal/GhosttyTerminalKernel.hpp"
#include "terminal/GhosttyC.hpp"

#include <kodosi_runtime.h>

#include <QByteArray>

#include <algorithm>
#include <iterator>
#include <limits>
#include <mutex>
#include <utility>

namespace kodosi {
namespace {

constexpr qsizetype maximumVisibleCells = 1'048'576;
constexpr std::size_t maximumGraphemeBytes = 4'096;
constexpr std::size_t maximumHyperlinkBytes = 8'192;

GhosttyTerminalKernel::Failure ghosttyFailure(QString operation, const GhosttyResult result)
{
    return {
        GhosttyTerminalKernel::Failure::Code::GhosttyRejected,
        QStringLiteral("%1 failed with Ghostty result %2.").arg(operation).arg(result),
    };
}

bool sameSubscription(
    const TerminalSubscription& left,
    const TerminalSubscription& right) noexcept
{
    return left.sessionId == right.sessionId
        && left.subscriptionId == right.subscriptionId
        && left.generation == right.generation;
}

QColor color(const GhosttyColorRgb value)
{
    return {value.r, value.g, value.b};
}

class TerminalHandle final {
public:
    TerminalHandle() = default;
    ~TerminalHandle()
    {
        ghostty_terminal_free(value);
    }

    TerminalHandle(const TerminalHandle&) = delete;
    TerminalHandle& operator=(const TerminalHandle&) = delete;

    GhosttyTerminal value = nullptr;
};

class RenderStateHandle final {
public:
    RenderStateHandle() = default;
    ~RenderStateHandle()
    {
        ghostty_render_state_free(value);
    }

    RenderStateHandle(const RenderStateHandle&) = delete;
    RenderStateHandle& operator=(const RenderStateHandle&) = delete;

    GhosttyRenderState value = nullptr;
};

class KeyEncoderHandle final {
public:
    KeyEncoderHandle() = default;
    ~KeyEncoderHandle()
    {
        ghostty_key_encoder_free(value);
    }

    KeyEncoderHandle(const KeyEncoderHandle&) = delete;
    KeyEncoderHandle& operator=(const KeyEncoderHandle&) = delete;

    GhosttyKeyEncoder value = nullptr;
};

class KeyEventHandle final {
public:
    KeyEventHandle() = default;
    ~KeyEventHandle()
    {
        ghostty_key_event_free(value);
    }

    KeyEventHandle(const KeyEventHandle&) = delete;
    KeyEventHandle& operator=(const KeyEventHandle&) = delete;

    GhosttyKeyEvent value = nullptr;
};

class MouseEncoderHandle final {
public:
    MouseEncoderHandle() = default;
    ~MouseEncoderHandle()
    {
        ghostty_mouse_encoder_free(value);
    }

    MouseEncoderHandle(const MouseEncoderHandle&) = delete;
    MouseEncoderHandle& operator=(const MouseEncoderHandle&) = delete;

    GhosttyMouseEncoder value = nullptr;
};

class MouseEventHandle final {
public:
    MouseEventHandle() = default;
    ~MouseEventHandle()
    {
        ghostty_mouse_event_free(value);
    }

    MouseEventHandle(const MouseEventHandle&) = delete;
    MouseEventHandle& operator=(const MouseEventHandle&) = delete;

    GhosttyMouseEvent value = nullptr;
};

class TrackedGridRefHandle final {
public:
    TrackedGridRefHandle() = default;
    ~TrackedGridRefHandle()
    {
        ghostty_tracked_grid_ref_free(value);
    }

    TrackedGridRefHandle(const TrackedGridRefHandle&) = delete;
    TrackedGridRefHandle& operator=(const TrackedGridRefHandle&) = delete;

    void reset(GhosttyTrackedGridRef next = nullptr)
    {
        ghostty_tracked_grid_ref_free(std::exchange(value, next));
    }

    GhosttyTrackedGridRef value = nullptr;
};

class RowIteratorHandle final {
public:
    RowIteratorHandle() = default;
    ~RowIteratorHandle()
    {
        ghostty_render_state_row_iterator_free(value);
    }

    RowIteratorHandle(const RowIteratorHandle&) = delete;
    RowIteratorHandle& operator=(const RowIteratorHandle&) = delete;

    GhosttyRenderStateRowIterator value = nullptr;
};

class RowCellsHandle final {
public:
    RowCellsHandle() = default;
    ~RowCellsHandle()
    {
        ghostty_render_state_row_cells_free(value);
    }

    RowCellsHandle(const RowCellsHandle&) = delete;
    RowCellsHandle& operator=(const RowCellsHandle&) = delete;

    GhosttyRenderStateRowCells value = nullptr;
};

using FrameResult = GhosttyTerminalKernel::Result;

std::optional<GhosttyKey> ghosttyKey(
    const TerminalKey key,
    const std::uint32_t unshiftedCodepoint)
{
    switch (key) {
    case TerminalKey::Backquote:
        return GHOSTTY_KEY_BACKQUOTE;
    case TerminalKey::Backslash:
        return GHOSTTY_KEY_BACKSLASH;
    case TerminalKey::BracketLeft:
        return GHOSTTY_KEY_BRACKET_LEFT;
    case TerminalKey::BracketRight:
        return GHOSTTY_KEY_BRACKET_RIGHT;
    case TerminalKey::Comma:
        return GHOSTTY_KEY_COMMA;
    case TerminalKey::Digit:
        if (unshiftedCodepoint >= '0' && unshiftedCodepoint <= '9') {
            return static_cast<GhosttyKey>(
                GHOSTTY_KEY_DIGIT_0 + unshiftedCodepoint - '0');
        }
        return std::nullopt;
    case TerminalKey::Equal:
        return GHOSTTY_KEY_EQUAL;
    case TerminalKey::Minus:
        return GHOSTTY_KEY_MINUS;
    case TerminalKey::Period:
        return GHOSTTY_KEY_PERIOD;
    case TerminalKey::Quote:
        return GHOSTTY_KEY_QUOTE;
    case TerminalKey::Semicolon:
        return GHOSTTY_KEY_SEMICOLON;
    case TerminalKey::Slash:
        return GHOSTTY_KEY_SLASH;
    case TerminalKey::Space:
        return GHOSTTY_KEY_SPACE;
    case TerminalKey::NumpadDigit:
        if (unshiftedCodepoint >= '0' && unshiftedCodepoint <= '9') {
            return static_cast<GhosttyKey>(
                GHOSTTY_KEY_NUMPAD_0 + unshiftedCodepoint - '0');
        }
        return std::nullopt;
    case TerminalKey::NumpadAdd:
        return GHOSTTY_KEY_NUMPAD_ADD;
    case TerminalKey::NumpadSubtract:
        return GHOSTTY_KEY_NUMPAD_SUBTRACT;
    case TerminalKey::NumpadMultiply:
        return GHOSTTY_KEY_NUMPAD_MULTIPLY;
    case TerminalKey::NumpadDivide:
        return GHOSTTY_KEY_NUMPAD_DIVIDE;
    case TerminalKey::NumpadDecimal:
        return GHOSTTY_KEY_NUMPAD_DECIMAL;
    case TerminalKey::NumpadEnter:
        return GHOSTTY_KEY_NUMPAD_ENTER;
    case TerminalKey::NumpadEqual:
        return GHOSTTY_KEY_NUMPAD_EQUAL;
    case TerminalKey::Enter:
        return GHOSTTY_KEY_ENTER;
    case TerminalKey::Backspace:
        return GHOSTTY_KEY_BACKSPACE;
    case TerminalKey::Tab:
        return GHOSTTY_KEY_TAB;
    case TerminalKey::Escape:
        return GHOSTTY_KEY_ESCAPE;
    case TerminalKey::Home:
        return GHOSTTY_KEY_HOME;
    case TerminalKey::End:
        return GHOSTTY_KEY_END;
    case TerminalKey::Insert:
        return GHOSTTY_KEY_INSERT;
    case TerminalKey::Delete:
        return GHOSTTY_KEY_DELETE;
    case TerminalKey::PageUp:
        return GHOSTTY_KEY_PAGE_UP;
    case TerminalKey::PageDown:
        return GHOSTTY_KEY_PAGE_DOWN;
    case TerminalKey::ArrowUp:
        return GHOSTTY_KEY_ARROW_UP;
    case TerminalKey::ArrowDown:
        return GHOSTTY_KEY_ARROW_DOWN;
    case TerminalKey::ArrowLeft:
        return GHOSTTY_KEY_ARROW_LEFT;
    case TerminalKey::ArrowRight:
        return GHOSTTY_KEY_ARROW_RIGHT;
    case TerminalKey::Function:
        if (unshiftedCodepoint >= 1 && unshiftedCodepoint <= 12) {
            return static_cast<GhosttyKey>(
                GHOSTTY_KEY_F1 + unshiftedCodepoint - 1);
        }
        return std::nullopt;
    case TerminalKey::Character:
        if (unshiftedCodepoint >= QLatin1Char('a').unicode()
            && unshiftedCodepoint <= QLatin1Char('z').unicode()) {
            return static_cast<GhosttyKey>(
                GHOSTTY_KEY_A + unshiftedCodepoint - QLatin1Char('a').unicode());
        }
        return std::nullopt;
    }
    return std::nullopt;
}

GhosttyKeyAction ghosttyAction(const TerminalKeyAction action)
{
    switch (action) {
    case TerminalKeyAction::Press:
        return GHOSTTY_KEY_ACTION_PRESS;
    case TerminalKeyAction::Repeat:
        return GHOSTTY_KEY_ACTION_REPEAT;
    case TerminalKeyAction::Release:
        return GHOSTTY_KEY_ACTION_RELEASE;
    }
    return GHOSTTY_KEY_ACTION_PRESS;
}

GhosttyMods ghosttyModifiers(const TerminalModifiers modifiers)
{
    GhosttyMods result = 0;
    if (modifiers.shift) {
        result |= GHOSTTY_MODS_SHIFT;
    }
    if (modifiers.control) {
        result |= GHOSTTY_MODS_CTRL;
    }
    if (modifiers.alt) {
        result |= GHOSTTY_MODS_ALT;
    }
    if (modifiers.superKey) {
        result |= GHOSTTY_MODS_SUPER;
    }
    return result;
}

GhosttyMouseAction ghosttyMouseAction(const TerminalMouseAction action)
{
    switch (action) {
    case TerminalMouseAction::Press:
        return GHOSTTY_MOUSE_ACTION_PRESS;
    case TerminalMouseAction::Release:
        return GHOSTTY_MOUSE_ACTION_RELEASE;
    case TerminalMouseAction::Motion:
        return GHOSTTY_MOUSE_ACTION_MOTION;
    }
    return GHOSTTY_MOUSE_ACTION_MOTION;
}

std::optional<GhosttyMouseButton> ghosttyMouseButton(
    const TerminalMouseButton button)
{
    switch (button) {
    case TerminalMouseButton::None:
        return std::nullopt;
    case TerminalMouseButton::Left:
        return GHOSTTY_MOUSE_BUTTON_LEFT;
    case TerminalMouseButton::Right:
        return GHOSTTY_MOUSE_BUTTON_RIGHT;
    case TerminalMouseButton::Middle:
        return GHOSTTY_MOUSE_BUTTON_MIDDLE;
    case TerminalMouseButton::WheelUp:
        return GHOSTTY_MOUSE_BUTTON_FOUR;
    case TerminalMouseButton::WheelDown:
        return GHOSTTY_MOUSE_BUTTON_FIVE;
    }
    return std::nullopt;
}

std::expected<QString, GhosttyTerminalKernel::Failure> readGrapheme(
    const GhosttyRenderStateRowCells cells)
{
    GhosttyBuffer buffer {};
    const auto query = ghostty_render_state_row_cells_get(
        cells,
        GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
        &buffer);
    if (query == GHOSTTY_SUCCESS && buffer.len == 0) {
        return QString {};
    }
    if (query != GHOSTTY_OUT_OF_SPACE || buffer.len == 0
        || buffer.len > maximumGraphemeBytes
        || buffer.len > static_cast<std::size_t>(std::numeric_limits<qsizetype>::max())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("grapheme query"), query));
    }

    QByteArray utf8(static_cast<qsizetype>(buffer.len), Qt::Uninitialized);
    buffer.ptr = reinterpret_cast<std::uint8_t*>(utf8.data());
    buffer.cap = static_cast<std::size_t>(utf8.size());
    buffer.len = 0;
    const auto result = ghostty_render_state_row_cells_get(
        cells,
        GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
        &buffer);
    if (result != GHOSTTY_SUCCESS || buffer.len > static_cast<std::size_t>(utf8.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("grapheme copy"), result));
    }
    utf8.resize(static_cast<qsizetype>(buffer.len));
    return QString::fromUtf8(utf8);
}

FrameResult extractFrame(
    const GhosttyTerminal terminal,
    const GhosttyRenderState renderState,
    const std::uint64_t nextSequence,
    const std::uint64_t viewportRevision,
    const std::uint64_t displayRevision)
{
    const auto update = ghostty_render_state_update(renderState, terminal);
    if (update != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("render-state update"), update));
    }

    std::uint16_t columns = 0;
    std::uint16_t rows = 0;
    GhosttyRenderStateColors colors = GHOSTTY_INIT_SIZED(GhosttyRenderStateColors);
    GhosttyRenderStateCursor cursor = GHOSTTY_INIT_SIZED(GhosttyRenderStateCursor);
    GhosttyTerminalScrollbar scrollbar {};
    const GhosttyRenderStateData keys[] {
        GHOSTTY_RENDER_STATE_DATA_COLS,
        GHOSTTY_RENDER_STATE_DATA_ROWS,
        GHOSTTY_RENDER_STATE_DATA_COLORS,
        GHOSTTY_RENDER_STATE_DATA_CURSOR,
    };
    void* values[] {&columns, &rows, &colors, &cursor};
    std::size_t written = 0;
    const auto stateResult = ghostty_render_state_get_multi(
        renderState,
        std::size(keys),
        keys,
        values,
        &written);
    if (stateResult != GHOSTTY_SUCCESS || written != std::size(keys)) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("render-state metadata"), stateResult));
    }
    const auto scrollbarResult = ghostty_terminal_get(
        terminal,
        GHOSTTY_TERMINAL_DATA_SCROLLBAR,
        &scrollbar);
    if (scrollbarResult != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("terminal scrollbar"), scrollbarResult));
    }

    const auto cellCount = static_cast<std::uint64_t>(columns) * rows;
    if (cellCount > static_cast<std::uint64_t>(maximumVisibleCells)) {
        return std::unexpected(GhosttyTerminalKernel::Failure {
            GhosttyTerminalKernel::Failure::Code::ResourceLimit,
            QStringLiteral("Terminal viewport exceeds the native renderer cell limit."),
        });
    }

    auto frame = std::make_shared<TerminalFrame>();
    frame->columns = columns;
    frame->rows = rows;
    frame->nextSequence = nextSequence;
    frame->viewportRevision = viewportRevision;
    frame->displayRevision = displayRevision;
    frame->foreground = color(colors.foreground);
    frame->background = color(colors.background);
    frame->cursorColor = color(
        colors.cursor_has_value ? colors.cursor : colors.foreground);
    frame->cursor = {
        .visible = cursor.visible && cursor.viewport_has_value,
        .blinking = cursor.blinking,
        .passwordInput = cursor.password_input,
        .wideTail = cursor.wide_tail,
        .column = cursor.viewport_has_value ? cursor.viewport_x : std::uint16_t {0},
        .row = cursor.viewport_has_value ? cursor.viewport_y : std::uint16_t {0},
        .visualStyle = cursor.visual_style,
    };
    frame->scroll = {
        .totalRows = scrollbar.total,
        .viewportOffset = scrollbar.offset,
        .viewportRows = scrollbar.len,
    };
    frame->cells.reserve(static_cast<qsizetype>(cellCount));

    RowIteratorHandle rowsIterator;
    auto result = ghostty_render_state_row_iterator_new(nullptr, &rowsIterator.value);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("row iterator creation"), result));
    }
    result = ghostty_render_state_get(
        renderState,
        GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
        &rowsIterator.value);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("row iterator binding"), result));
    }

    RowCellsHandle rowCells;
    result = ghostty_render_state_row_cells_new(nullptr, &rowCells.value);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("cell iterator creation"), result));
    }

    std::uint16_t row = 0;
    while (ghostty_render_state_row_iterator_next(rowsIterator.value)) {
        if (row >= rows) {
            return std::unexpected(GhosttyTerminalKernel::Failure {
                GhosttyTerminalKernel::Failure::Code::GhosttyRejected,
                QStringLiteral("Ghostty render state yielded too many rows."),
            });
        }

        GhosttyRenderStateRowSelection selection =
            GHOSTTY_INIT_SIZED(GhosttyRenderStateRowSelection);
        const auto selectionResult = ghostty_render_state_row_get(
            rowsIterator.value,
            GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION,
            &selection);
        if (selectionResult != GHOSTTY_SUCCESS && selectionResult != GHOSTTY_NO_VALUE) {
            return std::unexpected(
                ghosttyFailure(QStringLiteral("row selection"), selectionResult));
        }

        result = ghostty_render_state_row_get(
            rowsIterator.value,
            GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
            &rowCells.value);
        if (result != GHOSTTY_SUCCESS) {
            return std::unexpected(ghosttyFailure(QStringLiteral("row cells"), result));
        }

        std::uint16_t column = 0;
        while (ghostty_render_state_row_cells_next(rowCells.value)) {
            if (column >= columns) {
                return std::unexpected(GhosttyTerminalKernel::Failure {
                    GhosttyTerminalKernel::Failure::Code::GhosttyRejected,
                    QStringLiteral("Ghostty render state yielded too many columns."),
                });
            }

            auto grapheme = readGrapheme(rowCells.value);
            if (!grapheme) {
                return std::unexpected(grapheme.error());
            }

            GhosttyStyle style = GHOSTTY_INIT_SIZED(GhosttyStyle);
            result = ghostty_render_state_row_cells_get(
                rowCells.value,
                GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &style);
            if (result != GHOSTTY_SUCCESS) {
                return std::unexpected(ghosttyFailure(QStringLiteral("cell style"), result));
            }

            GhosttyColorRgb foreground {};
            const auto foregroundResult = ghostty_render_state_row_cells_get(
                rowCells.value,
                GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                &foreground);
            if (foregroundResult != GHOSTTY_SUCCESS && foregroundResult != GHOSTTY_INVALID_VALUE) {
                return std::unexpected(
                    ghosttyFailure(QStringLiteral("cell foreground"), foregroundResult));
            }

            GhosttyColorRgb background {};
            const auto backgroundResult = ghostty_render_state_row_cells_get(
                rowCells.value,
                GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                &background);
            if (backgroundResult != GHOSTTY_SUCCESS && backgroundResult != GHOSTTY_INVALID_VALUE) {
                return std::unexpected(
                    ghosttyFailure(QStringLiteral("cell background"), backgroundResult));
            }

            GhosttyCell rawCell = 0;
            result = ghostty_render_state_row_cells_get(
                rowCells.value,
                GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &rawCell);
            if (result != GHOSTTY_SUCCESS) {
                return std::unexpected(ghosttyFailure(QStringLiteral("raw cell"), result));
            }
            GhosttyCellWide wide = GHOSTTY_CELL_WIDE_NARROW;
            result = ghostty_cell_get(rawCell, GHOSTTY_CELL_DATA_WIDE, &wide);
            if (result != GHOSTTY_SUCCESS) {
                return std::unexpected(ghosttyFailure(QStringLiteral("cell width"), result));
            }

            auto cellForeground = foregroundResult == GHOSTTY_SUCCESS
                ? color(foreground)
                : frame->foreground;
            auto cellBackground = backgroundResult == GHOSTTY_SUCCESS
                ? color(background)
                : frame->background;
            if (style.inverse) {
                std::swap(cellForeground, cellBackground);
            }
            if (style.faint) {
                cellForeground.setAlpha(160);
            }

            frame->cells.push_back({
                .grapheme = style.invisible ? QString {} : *grapheme,
                .foreground = cellForeground,
                .background = cellBackground,
                .bold = style.bold,
                .italic = style.italic,
                .faint = style.faint,
                .inverse = style.inverse,
                .invisible = style.invisible,
                .strikethrough = style.strikethrough,
                .overline = style.overline,
                .selected = selectionResult == GHOSTTY_SUCCESS
                    && column >= selection.start_x
                    && column <= selection.end_x,
                .underline = style.underline,
                .width = wide == GHOSTTY_CELL_WIDE_WIDE ? std::uint8_t {2}
                    : (wide == GHOSTTY_CELL_WIDE_NARROW ? std::uint8_t {1}
                                                       : std::uint8_t {0}),
            });
            ++column;
        }
        if (column != columns) {
            return std::unexpected(GhosttyTerminalKernel::Failure {
                GhosttyTerminalKernel::Failure::Code::GhosttyRejected,
                QStringLiteral("Ghostty render row width does not match its metadata."),
            });
        }
        ++row;
    }
    if (row != rows || frame->cells.size() != static_cast<qsizetype>(cellCount)) {
        return std::unexpected(GhosttyTerminalKernel::Failure {
            GhosttyTerminalKernel::Failure::Code::GhosttyRejected,
            QStringLiteral("Ghostty render grid dimensions do not match its metadata."),
        });
    }

    result = ghostty_render_state_clean(renderState);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("render-state clean"), result));
    }
    return frame;
}

GhosttyTerminalKernel::ConfigureResult applySettings(
    const GhosttyTerminal terminal,
    const TerminalKernelSettings& settings)
{
    const auto cursorStyle = [&] {
        switch (settings.cursorStyle) {
        case TerminalCursorStyle::Block:
            return GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK;
        case TerminalCursorStyle::Bar:
            return GHOSTTY_TERMINAL_CURSOR_STYLE_BAR;
        case TerminalCursorStyle::Underline:
            return GHOSTTY_TERMINAL_CURSOR_STYLE_UNDERLINE;
        case TerminalCursorStyle::HollowBlock:
            return GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK_HOLLOW;
        }
        return GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK;
    }();
    auto result = ghostty_terminal_set(
        terminal,
        GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE,
        &cursorStyle);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("default cursor style"), result));
    }
    result = ghostty_terminal_set(
        terminal,
        GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,
        &settings.scrollbackBytes);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("scrollback byte limit"), result));
    }
    result = ghostty_terminal_set(
        terminal,
        GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK,
        &settings.cursorBlink);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("default cursor blink"), result));
    }
    result = ghostty_terminal_set(
        terminal,
        GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES,
        &settings.scrollbackLines);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("scrollback line limit"), result));
    }
    return {};
}

}

class GhosttyTerminalKernel::Impl final {
public:
    GhosttyResult refreshSelection()
    {
        if (selectionAnchor.value == nullptr || selectionEndpoint.value == nullptr) {
            return GHOSTTY_SUCCESS;
        }
        GhosttySelection selection = GHOSTTY_INIT_SIZED(GhosttySelection);
        auto result = ghostty_tracked_grid_ref_snapshot(
            selectionAnchor.value,
            &selection.start);
        if (result == GHOSTTY_SUCCESS) {
            result = ghostty_tracked_grid_ref_snapshot(
                selectionEndpoint.value,
                &selection.end);
        }
        if (result == GHOSTTY_NO_VALUE) {
            const auto clearResult = ghostty_terminal_set(
                terminal.value,
                GHOSTTY_TERMINAL_OPT_SELECTION,
                nullptr);
            selectionAnchor.reset();
            selectionEndpoint.reset();
            selectionRectangular = false;
            return clearResult;
        }
        if (result != GHOSTTY_SUCCESS) {
            return result;
        }
        selection.rectangle = selectionRectangular;
        return ghostty_terminal_set(
            terminal.value,
            GHOSTTY_TERMINAL_OPT_SELECTION,
            &selection);
    }

    mutable std::mutex mutex;
    TerminalHandle terminal;
    RenderStateHandle renderState;
    KeyEncoderHandle keyEncoder;
    MouseEncoderHandle mouseEncoder;
    TrackedGridRefHandle selectionAnchor;
    TrackedGridRefHandle selectionEndpoint;
    TerminalSubscription subscription;
    std::uint64_t nextSequence = 0;
    std::uint64_t viewportRevision = 0;
    std::uint64_t displayRevision = 0;
    Frame currentFrame;
    TerminalKernelSettings settings;
    bool selectionRectangular = false;
    bool admitted = false;
    bool failed = false;
};

GhosttyTerminalKernel::GhosttyTerminalKernel(TerminalKernelSettings settings)
    : m_impl(std::make_unique<Impl>())
{
    m_impl->settings = settings;
}

GhosttyTerminalKernel::~GhosttyTerminalKernel() = default;

GhosttyTerminalKernel::Result GhosttyTerminalKernel::installCheckpoint(
    const TerminalSemanticCheckpoint& checkpoint)
{
    if (checkpoint.rows == 0 || checkpoint.columns == 0
        || checkpoint.semanticJson.isEmpty()
        || checkpoint.semanticJson.size() > KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES
        || static_cast<std::uint64_t>(checkpoint.rows) * checkpoint.columns
            > static_cast<std::uint64_t>(maximumVisibleCells)) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Semantic checkpoint dimensions or payload are invalid."),
        });
    }

    std::scoped_lock lock(m_impl->mutex);
    if (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
        || m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }
    TerminalHandle candidateTerminal;
    auto result = ghostty_terminal_new(
        nullptr,
        &candidateTerminal.value,
        checkpoint.columns,
        checkpoint.rows);
    if (result != GHOSTTY_SUCCESS || candidateTerminal.value == nullptr) {
        return std::unexpected(ghosttyFailure(QStringLiteral("terminal creation"), result));
    }
    const std::uint64_t imageStorageLimit = 0;
    result = ghostty_terminal_set(
        candidateTerminal.value,
        GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT,
        &imageStorageLimit);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("disable Kitty image storage"), result));
    }
    const bool glyphProtocolEnabled = false;
    result = ghostty_terminal_set(
        candidateTerminal.value,
        GHOSTTY_TERMINAL_OPT_GLYPH_PROTOCOL,
        &glyphProtocolEnabled);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("disable Glyph Protocol"), result));
    }

    const GhosttyCheckpointRestoreOptions options = GHOSTTY_CHECKPOINT_RESTORE_OPTIONS_INIT;
    GhosttyCheckpointInfo info = GHOSTTY_INIT_SIZED(GhosttyCheckpointInfo);
    result = ghostty_checkpoint_restore(
        candidateTerminal.value,
        reinterpret_cast<const std::uint8_t*>(checkpoint.semanticJson.constData()),
        static_cast<std::size_t>(checkpoint.semanticJson.size()),
        &options,
        &info);
    if (result != GHOSTTY_SUCCESS || info.schema_version != GHOSTTY_CHECKPOINT_SCHEMA_VERSION
        || info.json_bytes != static_cast<std::size_t>(checkpoint.semanticJson.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("checkpoint restore"), result));
    }
    if (auto configured = applySettings(candidateTerminal.value, m_impl->settings);
        !configured) {
        return std::unexpected(std::move(configured.error()));
    }

    RenderStateHandle candidateRenderState;
    result = ghostty_render_state_new(nullptr, &candidateRenderState.value);
    if (result != GHOSTTY_SUCCESS || candidateRenderState.value == nullptr) {
        return std::unexpected(ghosttyFailure(QStringLiteral("render-state creation"), result));
    }
    const auto viewportRevision = m_impl->viewportRevision + 1;
    const auto displayRevision = m_impl->displayRevision + 1;
    auto candidateFrame = extractFrame(
        candidateTerminal.value,
        candidateRenderState.value,
        checkpoint.nextSequence,
        viewportRevision,
        displayRevision);
    if (!candidateFrame) {
        return candidateFrame;
    }
    if ((*candidateFrame)->rows != checkpoint.rows
        || (*candidateFrame)->columns != checkpoint.columns) {
        return std::unexpected(Failure {
            Failure::Code::GhosttyRejected,
            QStringLiteral("Semantic checkpoint dimensions do not match callback metadata."),
        });
    }
    KeyEncoderHandle candidateKeyEncoder;
    result = ghostty_key_encoder_new(nullptr, &candidateKeyEncoder.value);
    if (result != GHOSTTY_SUCCESS || candidateKeyEncoder.value == nullptr) {
        return std::unexpected(ghosttyFailure(QStringLiteral("key encoder creation"), result));
    }
    MouseEncoderHandle candidateMouseEncoder;
    result = ghostty_mouse_encoder_new(nullptr, &candidateMouseEncoder.value);
    if (result != GHOSTTY_SUCCESS || candidateMouseEncoder.value == nullptr) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("mouse encoder creation"), result));
    }

    std::swap(m_impl->terminal.value, candidateTerminal.value);
    std::swap(m_impl->renderState.value, candidateRenderState.value);
    std::swap(m_impl->keyEncoder.value, candidateKeyEncoder.value);
    std::swap(m_impl->mouseEncoder.value, candidateMouseEncoder.value);
    m_impl->selectionAnchor.reset();
    m_impl->selectionEndpoint.reset();
    m_impl->subscription = checkpoint.subscription;
    m_impl->nextSequence = checkpoint.nextSequence;
    m_impl->viewportRevision = viewportRevision;
    m_impl->displayRevision = displayRevision;
    m_impl->currentFrame = *candidateFrame;
    m_impl->selectionRectangular = false;
    m_impl->admitted = true;
    m_impl->failed = false;
    return m_impl->currentFrame;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::applyData(const TerminalData& data)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Raw terminal data requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(data.subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Raw terminal data belongs to a stale subscription."),
        });
    }
    if (data.sequence != m_impl->nextSequence
        || m_impl->nextSequence == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::SequenceMismatch,
            QStringLiteral("Raw terminal data is not the next contiguous frame."),
        });
    }
    if (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
        || m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    ghostty_terminal_vt_write(
        m_impl->terminal.value,
        reinterpret_cast<const std::uint8_t*>(data.bytes.constData()),
        static_cast<std::size_t>(data.bytes.size()));
    bool processingFailed = false;
    const auto status = ghostty_terminal_get(
        m_impl->terminal.value,
        GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR,
        &processingFailed);
    if (status != GHOSTTY_SUCCESS || processingFailed) {
        m_impl->failed = true;
        return std::unexpected(ghosttyFailure(QStringLiteral("terminal write"), status));
    }
    const auto selectionResult = m_impl->refreshSelection();
    if (selectionResult != GHOSTTY_SUCCESS) {
        m_impl->failed = true;
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection refresh"), selectionResult));
    }

    ++m_impl->nextSequence;
    ++m_impl->viewportRevision;
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::applyResize(
    const TerminalSubscription& subscription,
    const std::uint64_t atSequence,
    const std::uint16_t rows,
    const std::uint16_t columns)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal resize requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal resize belongs to a stale subscription."),
        });
    }
    if (rows == 0 || columns == 0 || atSequence != m_impl->nextSequence) {
        return std::unexpected(Failure {
            Failure::Code::SequenceMismatch,
            QStringLiteral("Terminal resize does not match the current sequence boundary."),
        });
    }
    if (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
        || m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    const auto result = ghostty_terminal_resize(
        m_impl->terminal.value,
        columns,
        rows,
        0,
        0);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("terminal resize"), result));
    }
    const auto selectionResult = m_impl->refreshSelection();
    if (selectionResult != GHOSTTY_SUCCESS) {
        m_impl->failed = true;
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection refresh"), selectionResult));
    }
    ++m_impl->viewportRevision;
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

std::expected<QByteArray, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::encodeKey(
    const TerminalSubscription& subscription,
    TerminalKeyEvent keyEvent)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal key encoding requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal key input belongs to a stale subscription."),
        });
    }
    const auto rawKey = ghosttyKey(keyEvent.key, keyEvent.unshiftedCodepoint);
    if (!rawKey) {
        return std::unexpected(Failure {
            Failure::Code::GhosttyRejected,
            QStringLiteral("Terminal key is not supported by the native encoder."),
        });
    }

    ghostty_key_encoder_setopt_from_terminal(
        m_impl->keyEncoder.value,
        m_impl->terminal.value);
    KeyEventHandle event;
    auto result = ghostty_key_event_new(nullptr, &event.value);
    if (result != GHOSTTY_SUCCESS || event.value == nullptr) {
        return std::unexpected(ghosttyFailure(QStringLiteral("key event creation"), result));
    }
    ghostty_key_event_set_action(event.value, ghosttyAction(keyEvent.action));
    ghostty_key_event_set_key(event.value, *rawKey);
    ghostty_key_event_set_mods(event.value, ghosttyModifiers(keyEvent.modifiers));
    const auto utf8 = keyEvent.text.toUtf8();
    if (!utf8.isEmpty()) {
        ghostty_key_event_set_utf8(
            event.value,
            utf8.constData(),
            static_cast<std::size_t>(utf8.size()));
    }
    if (keyEvent.unshiftedCodepoint != 0) {
        ghostty_key_event_set_unshifted_codepoint(
            event.value,
            keyEvent.unshiftedCodepoint);
    }

    std::size_t required = 0;
    result = ghostty_key_encoder_encode(
        m_impl->keyEncoder.value,
        event.value,
        nullptr,
        0,
        &required);
    if (result == GHOSTTY_SUCCESS && required == 0) {
        return QByteArray {};
    }
    if (result != GHOSTTY_OUT_OF_SPACE || required > maximumGraphemeBytes) {
        return std::unexpected(ghosttyFailure(QStringLiteral("key encoding size"), result));
    }
    if (required == 0) {
        return QByteArray {};
    }
    QByteArray encoded(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    result = ghostty_key_encoder_encode(
        m_impl->keyEncoder.value,
        event.value,
        encoded.data(),
        static_cast<std::size_t>(encoded.size()),
        &written);
    if (result != GHOSTTY_SUCCESS || written > static_cast<std::size_t>(encoded.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("key encoding"), result));
    }
    encoded.resize(static_cast<qsizetype>(written));
    return encoded;
}

std::expected<QByteArray, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::encodePaste(
    const TerminalSubscription& subscription,
    QByteArray text)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal paste requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal paste belongs to a stale subscription."),
        });
    }
    if (text.isEmpty()) {
        return QByteArray {};
    }

    GhosttyTerminalModeConfig bracketed {
        .mode = GHOSTTY_MODE_BRACKETED_PASTE,
        .value = false,
    };
    auto result = ghostty_terminal_get(
        m_impl->terminal.value,
        GHOSTTY_TERMINAL_DATA_MODE,
        &bracketed);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("bracketed paste mode"), result));
    }
    if (!bracketed.value
        && std::any_of(text.cbegin(), text.cend(), [](const char value) {
            const auto byte = static_cast<unsigned char>(value);
            return byte < 0x20 || byte == 0x7f;
        })) {
        return std::unexpected(Failure {
            Failure::Code::UnsafePaste,
            QStringLiteral(
                "Multiline or control-character paste requires bracketed paste mode; "
                "no input was sent."),
        });
    }

    std::size_t required = 0;
    result = ghostty_paste_encode(
        text.data(),
        static_cast<std::size_t>(text.size()),
        bracketed.value,
        nullptr,
        0,
        &required);
    if (result != GHOSTTY_OUT_OF_SPACE
        || required > static_cast<std::size_t>(KODOSI_MAX_FRAME_BYTES)
        || required > static_cast<std::size_t>(std::numeric_limits<qsizetype>::max())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("paste encoding size"), result));
    }
    QByteArray encoded(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    result = ghostty_paste_encode(
        text.data(),
        static_cast<std::size_t>(text.size()),
        bracketed.value,
        encoded.data(),
        static_cast<std::size_t>(encoded.size()),
        &written);
    if (result != GHOSTTY_SUCCESS
        || written > static_cast<std::size_t>(encoded.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("paste encoding"), result));
    }
    encoded.resize(static_cast<qsizetype>(written));
    return encoded;
}

std::expected<QByteArray, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::encodeMouse(
    const TerminalSubscription& subscription,
    TerminalMouseEvent mouseEvent)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal mouse input requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal mouse input belongs to a stale subscription."),
        });
    }
    if (m_impl->mouseEncoder.value == nullptr
        || mouseEvent.screenWidth == 0 || mouseEvent.screenHeight == 0
        || mouseEvent.cellWidth == 0 || mouseEvent.cellHeight == 0) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal mouse geometry is unavailable."),
        });
    }

    ghostty_mouse_encoder_setopt_from_terminal(
        m_impl->mouseEncoder.value,
        m_impl->terminal.value);
    const GhosttyMouseEncoderSize size {
        .size = sizeof(GhosttyMouseEncoderSize),
        .screen_width = mouseEvent.screenWidth,
        .screen_height = mouseEvent.screenHeight,
        .cell_width = mouseEvent.cellWidth,
        .cell_height = mouseEvent.cellHeight,
        .padding_top = 0,
        .padding_bottom = 0,
        .padding_right = 0,
        .padding_left = 0,
    };
    ghostty_mouse_encoder_setopt(
        m_impl->mouseEncoder.value,
        GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
        &size);
    ghostty_mouse_encoder_setopt(
        m_impl->mouseEncoder.value,
        GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
        &mouseEvent.anyButtonPressed);
    const bool trackLastCell = true;
    ghostty_mouse_encoder_setopt(
        m_impl->mouseEncoder.value,
        GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL,
        &trackLastCell);

    MouseEventHandle event;
    auto result = ghostty_mouse_event_new(nullptr, &event.value);
    if (result != GHOSTTY_SUCCESS || event.value == nullptr) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("mouse event creation"), result));
    }
    ghostty_mouse_event_set_action(
        event.value,
        ghosttyMouseAction(mouseEvent.action));
    if (const auto button = ghosttyMouseButton(mouseEvent.button)) {
        ghostty_mouse_event_set_button(event.value, *button);
    } else {
        ghostty_mouse_event_clear_button(event.value);
    }
    ghostty_mouse_event_set_mods(
        event.value,
        ghosttyModifiers(mouseEvent.modifiers));
    ghostty_mouse_event_set_position(
        event.value,
        {
            .x = mouseEvent.x,
            .y = mouseEvent.y,
        });

    std::size_t required = 0;
    result = ghostty_mouse_encoder_encode(
        m_impl->mouseEncoder.value,
        event.value,
        nullptr,
        0,
        &required);
    if (result == GHOSTTY_SUCCESS && required == 0) {
        return QByteArray {};
    }
    if (result != GHOSTTY_OUT_OF_SPACE
        || required > static_cast<std::size_t>(KODOSI_MAX_FRAME_BYTES)
        || required > static_cast<std::size_t>(
            std::numeric_limits<qsizetype>::max())) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("mouse encoding size"), result));
    }
    QByteArray encoded(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    result = ghostty_mouse_encoder_encode(
        m_impl->mouseEncoder.value,
        event.value,
        encoded.data(),
        static_cast<std::size_t>(encoded.size()),
        &written);
    if (result != GHOSTTY_SUCCESS
        || written > static_cast<std::size_t>(encoded.size())) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("mouse encoding"), result));
    }
    encoded.resize(static_cast<qsizetype>(written));
    return encoded;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::scrollViewport(
    const TerminalSubscription& subscription,
    const int rows)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal scrolling requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal scrolling belongs to a stale subscription."),
        });
    }
    if (rows == 0) {
        return m_impl->currentFrame;
    }
    const auto* frame = m_impl->currentFrame.get();
    if (frame != nullptr
        && ((rows < 0 && frame->scroll.viewportOffset == 0)
            || (rows > 0
                && frame->scroll.viewportOffset + frame->scroll.viewportRows
                    >= frame->scroll.totalRows))) {
        return m_impl->currentFrame;
    }
    if (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
        || m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    GhosttyTerminalScrollViewport scroll {
        .tag = GHOSTTY_SCROLL_VIEWPORT_DELTA,
        .value = {.delta = static_cast<intptr_t>(rows)},
    };
    ghostty_terminal_scroll_viewport(m_impl->terminal.value, scroll);
    const auto selectionResult = m_impl->refreshSelection();
    if (selectionResult != GHOSTTY_SUCCESS) {
        m_impl->failed = true;
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection refresh"), selectionResult));
    }
    ++m_impl->viewportRevision;
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::scrollViewportToBottom(
    const TerminalSubscription& subscription)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal scrolling requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal scrolling belongs to a stale subscription."),
        });
    }
    const auto* frame = m_impl->currentFrame.get();
    if (frame != nullptr
        && frame->scroll.viewportOffset + frame->scroll.viewportRows
            >= frame->scroll.totalRows) {
        return m_impl->currentFrame;
    }
    if (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
        || m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    GhosttyTerminalScrollViewport scroll {
        .tag = GHOSTTY_SCROLL_VIEWPORT_BOTTOM,
        .value = {},
    };
    ghostty_terminal_scroll_viewport(m_impl->terminal.value, scroll);
    const auto selectionResult = m_impl->refreshSelection();
    if (selectionResult != GHOSTTY_SUCCESS) {
        m_impl->failed = true;
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection refresh"), selectionResult));
    }
    ++m_impl->viewportRevision;
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

std::expected<QString, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::linkAt(
    const TerminalSubscription& subscription,
    const std::uint64_t viewportRevision,
    const std::uint16_t column,
    const std::uint16_t row)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal link lookup requires an admitted semantic checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal link lookup belongs to a stale subscription."),
        });
    }
    const auto* frame = m_impl->currentFrame.get();
    if (frame == nullptr || frame->viewportRevision != viewportRevision) {
        return std::unexpected(Failure {
            Failure::Code::StaleFrame,
            QStringLiteral("Terminal link lookup does not match the displayed frame."),
        });
    }
    if (column >= frame->columns || row >= frame->rows) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal link lookup point is outside the viewport."),
        });
    }

    GhosttyPoint point {};
    point.tag = GHOSTTY_POINT_TAG_VIEWPORT;
    point.value.coordinate = {column, row};
    GhosttyGridRef reference = GHOSTTY_INIT_SIZED(GhosttyGridRef);
    auto result = ghostty_terminal_grid_ref(
        m_impl->terminal.value,
        point,
        &reference);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("link grid reference"), result));
    }

    std::size_t required = 0;
    result = ghostty_grid_ref_hyperlink_uri(
        &reference,
        nullptr,
        0,
        &required);
    if (result == GHOSTTY_SUCCESS && required == 0) {
        return QString {};
    }
    if (result != GHOSTTY_OUT_OF_SPACE || required == 0
        || required > maximumHyperlinkBytes
        || required > static_cast<std::size_t>(std::numeric_limits<qsizetype>::max())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("link URI size"), result));
    }
    QByteArray uri(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    result = ghostty_grid_ref_hyperlink_uri(
        &reference,
        reinterpret_cast<std::uint8_t*>(uri.data()),
        static_cast<std::size_t>(uri.size()),
        &written);
    if (result != GHOSTTY_SUCCESS || written > static_cast<std::size_t>(uri.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("link URI copy"), result));
    }
    uri.resize(static_cast<qsizetype>(written));
    const auto value = QString::fromUtf8(uri);
    if (value.toUtf8() != uri) {
        return std::unexpected(Failure {
            Failure::Code::GhosttyRejected,
            QStringLiteral("Terminal link URI is not valid UTF-8."),
        });
    }
    return value;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::beginSelection(
    const TerminalSubscription& subscription,
    const std::uint64_t viewportRevision,
    const std::uint16_t column,
    const std::uint16_t row,
    const bool rectangular)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal selection requires an admitted checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal selection belongs to a stale subscription."),
        });
    }
    const auto* frame = m_impl->currentFrame.get();
    if (frame == nullptr || frame->viewportRevision != viewportRevision) {
        return std::unexpected(Failure {
            Failure::Code::StaleFrame,
            QStringLiteral("Terminal selection does not match the displayed frame."),
        });
    }
    if (column >= frame->columns || row >= frame->rows) {
        return std::unexpected(Failure {
            Failure::Code::HitTestRace,
            QStringLiteral("Terminal selection point no longer matches the viewport."),
        });
    }
    if (m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    GhosttyPoint point {};
    point.tag = GHOSTTY_POINT_TAG_VIEWPORT;
    point.value.coordinate = {column, row};
    TrackedGridRefHandle anchor;
    auto result = ghostty_terminal_grid_ref_track(
        m_impl->terminal.value,
        point,
        &anchor.value);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection anchor"), result));
    }
    TrackedGridRefHandle endpoint;
    result = ghostty_terminal_grid_ref_track(
        m_impl->terminal.value,
        point,
        &endpoint.value);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection endpoint"), result));
    }
    m_impl->selectionAnchor.reset(std::exchange(anchor.value, nullptr));
    m_impl->selectionEndpoint.reset(std::exchange(endpoint.value, nullptr));
    m_impl->selectionRectangular = rectangular;
    result = m_impl->refreshSelection();
    if (result != GHOSTTY_SUCCESS) {
        m_impl->selectionAnchor.reset();
        m_impl->selectionEndpoint.reset();
        m_impl->selectionRectangular = false;
        return std::unexpected(ghosttyFailure(QStringLiteral("selection install"), result));
    }
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::updateSelection(
    const TerminalSubscription& subscription,
    const std::uint64_t viewportRevision,
    const std::uint16_t column,
    const std::uint16_t row)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal selection requires an admitted checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal selection belongs to a stale subscription."),
        });
    }
    const auto* frame = m_impl->currentFrame.get();
    if (frame == nullptr || frame->viewportRevision != viewportRevision) {
        return std::unexpected(Failure {
            Failure::Code::StaleFrame,
            QStringLiteral("Terminal selection does not match the displayed frame."),
        });
    }
    if (m_impl->selectionAnchor.value == nullptr
        || m_impl->selectionEndpoint.value == nullptr
        || column >= frame->columns || row >= frame->rows) {
        return std::unexpected(Failure {
            Failure::Code::HitTestRace,
            QStringLiteral("Terminal selection point or anchor is no longer available."),
        });
    }
    if (m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }

    GhosttyPoint endpoint {};
    endpoint.tag = GHOSTTY_POINT_TAG_VIEWPORT;
    endpoint.value.coordinate = {column, row};
    auto result = ghostty_tracked_grid_ref_set(
        m_impl->selectionEndpoint.value,
        m_impl->terminal.value,
        endpoint);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("selection grid reference"), result));
    }
    result = m_impl->refreshSelection();
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("selection install"), result));
    }
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

GhosttyTerminalKernel::Result GhosttyTerminalKernel::clearSelection(
    const TerminalSubscription& subscription)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal selection requires an admitted checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal selection belongs to a stale subscription."),
        });
    }
    if (m_impl->displayRevision == std::numeric_limits<std::uint64_t>::max()) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal display revision is exhausted."),
        });
    }
    const auto result = ghostty_terminal_set(
        m_impl->terminal.value,
        GHOSTTY_TERMINAL_OPT_SELECTION,
        nullptr);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(ghosttyFailure(QStringLiteral("selection clear"), result));
    }
    m_impl->selectionAnchor.reset();
    m_impl->selectionEndpoint.reset();
    m_impl->selectionRectangular = false;
    ++m_impl->displayRevision;
    auto nextFrame = extractFrame(
        m_impl->terminal.value,
        m_impl->renderState.value,
        m_impl->nextSequence,
        m_impl->viewportRevision,
        m_impl->displayRevision);
    if (!nextFrame) {
        m_impl->failed = true;
        return nextFrame;
    }
    m_impl->currentFrame = *nextFrame;
    return m_impl->currentFrame;
}

std::expected<QString, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::selectedText(const TerminalSubscription& subscription)
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal selection requires an admitted checkpoint."),
        });
    }
    if (!sameSubscription(subscription, m_impl->subscription)) {
        return std::unexpected(Failure {
            Failure::Code::StaleSubscription,
            QStringLiteral("Terminal selection belongs to a stale subscription."),
        });
    }
    GhosttyTerminalSelectionFormatOptions options =
        GHOSTTY_INIT_SIZED(GhosttyTerminalSelectionFormatOptions);
    kodosi_ghostty_selection_emit(&options, GHOSTTY_FORMATTER_FORMAT_PLAIN);
    options.unwrap = true;
    options.trim = true;
    std::size_t required = 0;
    auto result = ghostty_terminal_selection_format_buf(
        m_impl->terminal.value,
        options,
        nullptr,
        0,
        &required);
    if (result == GHOSTTY_NO_VALUE) {
        return QString {};
    }
    if (result != GHOSTTY_OUT_OF_SPACE || required > KODOSI_MAX_FRAME_BYTES) {
        return std::unexpected(ghosttyFailure(QStringLiteral("selection size"), result));
    }
    QByteArray bytes(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    result = ghostty_terminal_selection_format_buf(
        m_impl->terminal.value,
        options,
        reinterpret_cast<std::uint8_t*>(bytes.data()),
        static_cast<std::size_t>(bytes.size()),
        &written);
    if (result != GHOSTTY_SUCCESS || written > static_cast<std::size_t>(bytes.size())) {
        return std::unexpected(ghosttyFailure(QStringLiteral("selection copy"), result));
    }
    bytes.resize(static_cast<qsizetype>(written));
    return QString::fromUtf8(bytes);
}

GhosttyTerminalKernel::ConfigureResult GhosttyTerminalKernel::configure(
    TerminalKernelSettings settings)
{
    if (settings.scrollbackLines < 100 || settings.scrollbackLines > 100'000
        || settings.scrollbackBytes
            != terminalScrollbackByteBudget(settings.scrollbackLines)) {
        return std::unexpected(Failure {
            Failure::Code::ResourceLimit,
            QStringLiteral("Terminal settings are outside supported ranges."),
        });
    }
    std::scoped_lock lock(m_impl->mutex);
    if (settings == m_impl->settings) {
        return std::optional<Frame> {};
    }
    const auto scrollbackChanged =
        settings.scrollbackLines != m_impl->settings.scrollbackLines
        || settings.scrollbackBytes != m_impl->settings.scrollbackBytes;
    if (m_impl->admitted && !m_impl->failed) {
        if (scrollbackChanged
            && (m_impl->viewportRevision == std::numeric_limits<std::uint64_t>::max()
                || m_impl->displayRevision
                    == std::numeric_limits<std::uint64_t>::max())) {
            return std::unexpected(Failure {
                Failure::Code::ResourceLimit,
                QStringLiteral("Terminal display revision is exhausted."),
            });
        }
        if (auto configured = applySettings(m_impl->terminal.value, settings);
            !configured) {
            m_impl->failed = true;
            return std::unexpected(std::move(configured.error()));
        }
        if (scrollbackChanged) {
            const auto selectionResult = m_impl->refreshSelection();
            if (selectionResult != GHOSTTY_SUCCESS) {
                m_impl->failed = true;
                return std::unexpected(
                    ghosttyFailure(QStringLiteral("selection refresh"), selectionResult));
            }
            ++m_impl->viewportRevision;
            ++m_impl->displayRevision;
            auto nextFrame = extractFrame(
                m_impl->terminal.value,
                m_impl->renderState.value,
                m_impl->nextSequence,
                m_impl->viewportRevision,
                m_impl->displayRevision);
            if (!nextFrame) {
                m_impl->failed = true;
                return std::unexpected(std::move(nextFrame.error()));
            }
            m_impl->settings = settings;
            m_impl->currentFrame = *nextFrame;
            return std::optional<Frame> {m_impl->currentFrame};
        }
    }
    m_impl->settings = settings;
    return std::optional<Frame> {};
}

std::expected<std::size_t, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::scrollbackLimitLines() const
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Scrollback diagnostics require an admitted checkpoint."),
        });
    }
    std::size_t lines = 0;
    const auto result = ghostty_terminal_get(
        m_impl->terminal.value,
        GHOSTTY_TERMINAL_DATA_SCROLLBACK_MAX_LINES,
        &lines);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("scrollback line limit query"), result));
    }
    return lines;
}

std::expected<std::size_t, GhosttyTerminalKernel::Failure>
GhosttyTerminalKernel::scrollbackLimitBytes() const
{
    std::scoped_lock lock(m_impl->mutex);
    if (!m_impl->admitted || m_impl->failed) {
        return std::unexpected(Failure {
            Failure::Code::MissingCheckpoint,
            QStringLiteral("Terminal has no admitted checkpoint."),
        });
    }
    std::size_t bytes = 0;
    const auto result = ghostty_terminal_get(
        m_impl->terminal.value,
        GHOSTTY_TERMINAL_DATA_SCROLLBACK_MAX_BYTES,
        &bytes);
    if (result != GHOSTTY_SUCCESS) {
        return std::unexpected(
            ghosttyFailure(QStringLiteral("scrollback byte limit query"), result));
    }
    return bytes;
}

GhosttyTerminalKernel::Frame GhosttyTerminalKernel::frame() const
{
    std::scoped_lock lock(m_impl->mutex);
    return m_impl->currentFrame;
}

}
