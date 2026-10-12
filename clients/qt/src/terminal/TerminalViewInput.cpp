#include "terminal/TerminalView.hpp"
#include "terminal/TerminalLink.hpp"

#include <kodosi_runtime.h>

#include <QClipboard>
#include <QCursor>
#include <QDesktopServices>
#include <QGuiApplication>
#include <QHoverEvent>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMouseEvent>
#include <QTimer>
#include <QWheelEvent>

#include <algorithm>
#include <cmath>
#include <limits>

namespace kodosi {
namespace {

constexpr qsizetype maximumQueuedInputBytes = 1024 * 1024;
constexpr std::size_t maximumQueuedInputFrames = 1024;
constexpr int maximumWheelRowsPerEvent = 100;

std::uint32_t unshiftedCodepoint(const int key)
{
    if (key >= Qt::Key_A && key <= Qt::Key_Z) {
        return QLatin1Char('a').unicode() + key - Qt::Key_A;
    }
    if (key >= Qt::Key_0 && key <= Qt::Key_9) {
        return QLatin1Char('0').unicode() + key - Qt::Key_0;
    }
    if (key >= Qt::Key_F1 && key <= Qt::Key_F12) {
        return static_cast<std::uint32_t>(key - Qt::Key_F1 + 1);
    }
    switch (key) {
    case Qt::Key_ParenRight:
        return '0';
    case Qt::Key_Exclam:
        return '1';
    case Qt::Key_At:
        return '2';
    case Qt::Key_NumberSign:
        return '3';
    case Qt::Key_Dollar:
        return '4';
    case Qt::Key_Percent:
        return '5';
    case Qt::Key_AsciiCircum:
        return '6';
    case Qt::Key_Ampersand:
        return '7';
    case Qt::Key_Asterisk:
        return '8';
    case Qt::Key_ParenLeft:
        return '9';
    case Qt::Key_QuoteLeft:
    case Qt::Key_AsciiTilde:
        return '`';
    case Qt::Key_Backslash:
    case Qt::Key_Bar:
        return '\\';
    case Qt::Key_BracketLeft:
    case Qt::Key_BraceLeft:
        return '[';
    case Qt::Key_BracketRight:
    case Qt::Key_BraceRight:
        return ']';
    case Qt::Key_Comma:
    case Qt::Key_Less:
        return ',';
    case Qt::Key_Equal:
    case Qt::Key_Plus:
        return '=';
    case Qt::Key_Minus:
    case Qt::Key_Underscore:
        return '-';
    case Qt::Key_Period:
    case Qt::Key_Greater:
        return '.';
    case Qt::Key_Apostrophe:
    case Qt::Key_QuoteDbl:
        return '\'';
    case Qt::Key_Semicolon:
    case Qt::Key_Colon:
        return ';';
    case Qt::Key_Slash:
    case Qt::Key_Question:
        return '/';
    case Qt::Key_Space:
        return ' ';
    default:
        return 0;
    }
}

TerminalModifiers modifiers(const Qt::KeyboardModifiers value)
{
    return {
        .shift = value.testFlag(Qt::ShiftModifier),
        .control = value.testFlag(Qt::ControlModifier),
        .alt = value.testFlag(Qt::AltModifier),
        .superKey = value.testFlag(Qt::MetaModifier),
    };
}

TerminalMouseButton mouseButton(const Qt::MouseButton button)
{
    switch (button) {
    case Qt::LeftButton:
        return TerminalMouseButton::Left;
    case Qt::RightButton:
        return TerminalMouseButton::Right;
    case Qt::MiddleButton:
        return TerminalMouseButton::Middle;
    default:
        return TerminalMouseButton::None;
    }
}

std::optional<TerminalKey> terminalKey(const QKeyEvent* event)
{
    const auto key = event->key();
    if (event->modifiers().testFlag(Qt::KeypadModifier)) {
        if (key >= Qt::Key_0 && key <= Qt::Key_9) {
            return TerminalKey::NumpadDigit;
        }
        switch (key) {
        case Qt::Key_Plus:
            return TerminalKey::NumpadAdd;
        case Qt::Key_Minus:
            return TerminalKey::NumpadSubtract;
        case Qt::Key_Asterisk:
            return TerminalKey::NumpadMultiply;
        case Qt::Key_Slash:
            return TerminalKey::NumpadDivide;
        case Qt::Key_Period:
        case Qt::Key_Comma:
            return TerminalKey::NumpadDecimal;
        case Qt::Key_Enter:
        case Qt::Key_Return:
            return TerminalKey::NumpadEnter;
        case Qt::Key_Equal:
            return TerminalKey::NumpadEqual;
        default:
            break;
        }
    }
    switch (key) {
    case Qt::Key_QuoteLeft:
    case Qt::Key_AsciiTilde:
        return TerminalKey::Backquote;
    case Qt::Key_Backslash:
    case Qt::Key_Bar:
        return TerminalKey::Backslash;
    case Qt::Key_BracketLeft:
    case Qt::Key_BraceLeft:
        return TerminalKey::BracketLeft;
    case Qt::Key_BracketRight:
    case Qt::Key_BraceRight:
        return TerminalKey::BracketRight;
    case Qt::Key_Comma:
    case Qt::Key_Less:
        return TerminalKey::Comma;
    case Qt::Key_0:
    case Qt::Key_ParenRight:
    case Qt::Key_1:
    case Qt::Key_Exclam:
    case Qt::Key_2:
    case Qt::Key_At:
    case Qt::Key_3:
    case Qt::Key_NumberSign:
    case Qt::Key_4:
    case Qt::Key_Dollar:
    case Qt::Key_5:
    case Qt::Key_Percent:
    case Qt::Key_6:
    case Qt::Key_AsciiCircum:
    case Qt::Key_7:
    case Qt::Key_Ampersand:
    case Qt::Key_8:
    case Qt::Key_Asterisk:
    case Qt::Key_9:
    case Qt::Key_ParenLeft:
        return TerminalKey::Digit;
    case Qt::Key_Equal:
    case Qt::Key_Plus:
        return TerminalKey::Equal;
    case Qt::Key_Minus:
    case Qt::Key_Underscore:
        return TerminalKey::Minus;
    case Qt::Key_Period:
    case Qt::Key_Greater:
        return TerminalKey::Period;
    case Qt::Key_Apostrophe:
    case Qt::Key_QuoteDbl:
        return TerminalKey::Quote;
    case Qt::Key_Semicolon:
    case Qt::Key_Colon:
        return TerminalKey::Semicolon;
    case Qt::Key_Slash:
    case Qt::Key_Question:
        return TerminalKey::Slash;
    case Qt::Key_Space:
        return TerminalKey::Space;
    case Qt::Key_Return:
    case Qt::Key_Enter:
        return TerminalKey::Enter;
    case Qt::Key_Backspace:
        return TerminalKey::Backspace;
    case Qt::Key_Tab:
    case Qt::Key_Backtab:
        return TerminalKey::Tab;
    case Qt::Key_Escape:
        return TerminalKey::Escape;
    case Qt::Key_Home:
        return TerminalKey::Home;
    case Qt::Key_End:
        return TerminalKey::End;
    case Qt::Key_Insert:
        return TerminalKey::Insert;
    case Qt::Key_Delete:
        return TerminalKey::Delete;
    case Qt::Key_PageUp:
        return TerminalKey::PageUp;
    case Qt::Key_PageDown:
        return TerminalKey::PageDown;
    case Qt::Key_Up:
        return TerminalKey::ArrowUp;
    case Qt::Key_Down:
        return TerminalKey::ArrowDown;
    case Qt::Key_Left:
        return TerminalKey::ArrowLeft;
    case Qt::Key_Right:
        return TerminalKey::ArrowRight;
    default:
        if (key >= Qt::Key_F1 && key <= Qt::Key_F12) {
            return TerminalKey::Function;
        }
        return key >= Qt::Key_A && key <= Qt::Key_Z
            ? std::optional<TerminalKey> {TerminalKey::Character}
            : std::nullopt;
    }
}

bool isPasteShortcut(const QKeyEvent* event)
{
    const auto modifiers = event->modifiers();
    const auto terminalShortcut = event->key() == Qt::Key_V
        && modifiers == (Qt::ControlModifier | Qt::ShiftModifier);
    const auto platformShortcut = event->matches(QKeySequence::Paste)
        && !modifiers.testFlag(Qt::ControlModifier);
    return terminalShortcut || platformShortcut;
}

}

bool TerminalView::event(QEvent* event)
{
    if (event->type() == QEvent::ShortcutOverride && hasActiveFocus() && interactionAvailable()) {
        const auto* key = static_cast<QKeyEvent*>(event);
        const auto modifiers = key->modifiers();
        if (modifiers == Qt::ControlModifier
            || modifiers == Qt::AltModifier
            || modifiers == Qt::NoModifier) {
            event->accept();
            return true;
        }
    }
    return QQuickItem::event(event);
}

void TerminalView::keyPressEvent(QKeyEvent* event)
{
    if (!interactionAvailable() || m_runtime == nullptr || m_registry == nullptr || !m_terminalReady) {
        QQuickItem::keyPressEvent(event);
        return;
    }
    if (event->key() == Qt::Key_C
        && event->modifiers().testFlags(Qt::ControlModifier | Qt::ShiftModifier)) {
        m_copyShortcutActive = true;
        (void)copySelection();
        event->accept();
        return;
    }
    if (isPasteShortcut(event)) {
        if (!event->isAutoRepeat()) {
            m_pasteShortcutKey = event->key();
        }
        if (m_canSendInput && !event->isAutoRepeat()) {
            pasteClipboard();
        }
        event->accept();
        return;
    }
    if (!m_canSendInput) {
        QQuickItem::keyPressEvent(event);
        return;
    }
    const auto text = event->text();
    const auto key = terminalKey(event);
    if (key && sendKey(
            event,
            event->isAutoRepeat() ? TerminalKeyAction::Repeat : TerminalKeyAction::Press)) {
        event->accept();
        return;
    }
    if (!text.isEmpty()
        && !event->modifiers().testAnyFlags(
            Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)
        && event->key() != Qt::Key_Return
        && event->key() != Qt::Key_Enter
        && event->key() != Qt::Key_Tab
        && event->key() != Qt::Key_Backtab) {
        sendText(text);
        event->accept();
        return;
    }
    if (key && sendKey(
            event,
            event->isAutoRepeat() ? TerminalKeyAction::Repeat : TerminalKeyAction::Press)) {
        event->accept();
        return;
    }
    QQuickItem::keyPressEvent(event);
}

void TerminalView::keyReleaseEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_C && m_copyShortcutActive) {
        m_copyShortcutActive = false;
        event->accept();
        return;
    }
    if (m_pasteShortcutKey && event->key() == *m_pasteShortcutKey) {
        m_pasteShortcutKey.reset();
        event->accept();
        return;
    }
    if (event->isAutoRepeat()) {
        event->accept();
        return;
    }
    if (!interactionAvailable() || !m_terminalReady || !m_canSendInput) {
        QQuickItem::keyReleaseEvent(event);
        return;
    }
    if (sendKey(event, TerminalKeyAction::Release)) {
        event->accept();
        return;
    }
    QQuickItem::keyReleaseEvent(event);
}

void TerminalView::inputMethodEvent(QInputMethodEvent* event)
{
    if (!interactionAvailable() || !m_terminalReady || !m_canSendInput) {
        QQuickItem::inputMethodEvent(event);
        return;
    }
    if (!event->commitString().isEmpty()) {
        sendText(event->commitString());
    }
    if (m_preedit != event->preeditString()) {
        m_preedit = event->preeditString();
        update();
    }
    event->accept();
}

QVariant TerminalView::inputMethodQuery(const Qt::InputMethodQuery query) const
{
    switch (query) {
    case Qt::ImEnabled:
        return interactionAvailable() && m_runtime != nullptr && m_terminalReady
            && m_canSendInput;
    case Qt::ImFont:
        return m_font;
    case Qt::ImCursorRectangle:
        if (m_frame) {
            const auto cell = TerminalRasterizer::cellSize(m_font, m_lineHeight);
            const auto scale = viewportScale();
            return QRectF(viewportOffset() + QPointF(
                cell.width() * m_frame->cursor.column,
                cell.height() * m_frame->cursor.row) * scale,
                cell * scale);
        }
        return QRectF {};
    case Qt::ImCursorPosition:
    case Qt::ImAnchorPosition:
        return 0;
    case Qt::ImSurroundingText:
    case Qt::ImCurrentSelection:
        return QString {};
    default:
        return QQuickItem::inputMethodQuery(query);
    }
}

void TerminalView::mousePressEvent(QMouseEvent* event)
{
    if (!interactionAvailable() || !m_terminalReady) {
        QQuickItem::mousePressEvent(event);
        return;
    }
    forceActiveFocus(Qt::MouseFocusReason);
    if (m_canSendInput
        && !event->modifiers().testFlag(Qt::ShiftModifier)
        && sendMouseEvent(
            TerminalMouseAction::Press,
            mouseButton(event->button()),
            event->position(),
            event->modifiers(),
            event->buttons() != Qt::NoButton)) {
        m_reportedMouseButton = event->button();
        m_pressedLink.reset();
        m_selecting = false;
        event->accept();
        return;
    }
    if (event->button() == Qt::RightButton) {
        emit contextMenuRequested(
            event->position().x(),
            event->position().y());
        event->accept();
        return;
    }
    if (event->button() != Qt::LeftButton) {
        QQuickItem::mousePressEvent(event);
        return;
    }
    const auto cell = terminalCellAt(event->position());
    if (!cell) {
        QQuickItem::mousePressEvent(event);
        return;
    }
    if (m_canSendInput && event->modifiers() == Qt::NoModifier) {
        if (auto link = linkAtCell(*cell, true)) {
            m_pressedLink = std::move(*link);
            m_selecting = false;
            event->accept();
            return;
        }
    }
    m_pressedLink.reset();
    beginSelection(*cell);
    event->accept();
}

void TerminalView::mouseMoveEvent(QMouseEvent* event)
{
    if (m_canSendInput
        && !event->modifiers().testFlag(Qt::ShiftModifier)
        && m_reportedMouseButton != Qt::NoButton
        && sendMouseEvent(
            TerminalMouseAction::Motion,
            mouseButton(m_reportedMouseButton),
            event->position(),
            event->modifiers(),
            event->buttons() != Qt::NoButton)) {
        event->accept();
        return;
    }
    if (m_pressedLink && event->buttons().testFlag(Qt::LeftButton)) {
        event->accept();
        return;
    }
    if (!m_selecting || !event->buttons().testFlag(Qt::LeftButton)) {
        QQuickItem::mouseMoveEvent(event);
        return;
    }
    if (const auto cell = terminalCellAt(
            event->position(),
            CellHitTest::ClampToViewport)) {
        updateSelection(*cell);
    }
    event->accept();
}

void TerminalView::mouseReleaseEvent(QMouseEvent* event)
{
    if (event->button() == m_reportedMouseButton) {
        (void)sendMouseEvent(
            TerminalMouseAction::Release,
            mouseButton(event->button()),
            event->position(),
            event->modifiers(),
            false);
        m_reportedMouseButton = Qt::NoButton;
        event->accept();
        return;
    }
    if (event->button() == Qt::LeftButton && m_pressedLink) {
        const auto pressed = std::exchange(m_pressedLink, std::nullopt);
        const auto cell = terminalCellAt(
            event->position(),
            CellHitTest::Strict);
        const auto released = cell ? linkAtCell(*cell, true) : std::nullopt;
        if (released && *released == *pressed
            && !QDesktopServices::openUrl(*released)) {
            emit operationError(QStringLiteral("The terminal link could not be opened."));
        }
        event->accept();
        return;
    }
    if (event->button() != Qt::LeftButton || !m_selecting) {
        QQuickItem::mouseReleaseEvent(event);
        return;
    }
    if (const auto cell = terminalCellAt(
            event->position(),
            CellHitTest::ClampToViewport)) {
        updateSelection(*cell);
    }
    m_selecting = false;
    event->accept();
}

void TerminalView::hoverMoveEvent(QHoverEvent* event)
{
    if (m_canSendInput
        && sendMouseEvent(
            TerminalMouseAction::Motion,
            TerminalMouseButton::None,
            event->position(),
            event->modifiers(),
            false)) {
        clearHoverLink();
        event->accept();
        return;
    }
    updateHoverLink(event->position());
    event->accept();
}

void TerminalView::hoverLeaveEvent(QHoverEvent* event)
{
    clearHoverLink();
    event->accept();
}

void TerminalView::wheelEvent(QWheelEvent* event)
{
    if (!interactionAvailable() || !m_terminalReady || m_registry == nullptr) {
        QQuickItem::wheelEvent(event);
        return;
    }
    const auto delta = event->pixelDelta().y() != 0
        ? event->pixelDelta().y()
        : event->angleDelta().y();
    if (m_canSendInput && delta != 0
        && sendMouseEvent(
            TerminalMouseAction::Press,
            delta > 0
                ? TerminalMouseButton::WheelUp
                : TerminalMouseButton::WheelDown,
            event->position(),
            event->modifiers(),
            false)) {
        event->accept();
        return;
    }
    const auto rows = wheelRows(event);
    if (rows != 0) {
        auto scrolled = m_registry->scrollViewport(surfaceIdentity(), rows);
        if (!scrolled) {
            emit operationError(scrolled.error().message);
        }
    }
    if (event->pixelDelta().y() != 0 || event->angleDelta().y() != 0) {
        event->accept();
        return;
    }
    QQuickItem::wheelEvent(event);
}

bool TerminalView::sendMouseEvent(
    const TerminalMouseAction action,
    const TerminalMouseButton button,
    const QPointF& position,
    const Qt::KeyboardModifiers keyboardModifiers,
    const bool anyButtonPressed)
{
    if (!interactionAvailable() || !m_canSendInput || m_registry == nullptr || m_runtime == nullptr
        || !m_terminalReady) {
        return false;
    }
    const auto cellSize =
        TerminalRasterizer::cellSize(m_font, m_lineHeight);
    if (cellSize.width() <= 0 || cellSize.height() <= 0
        || width() <= 0 || height() <= 0) {
        emit operationError(
            QStringLiteral("Terminal mouse geometry is unavailable."));
        return true;
    }
    const auto point = gridPoint(position);
    const auto grid = gridSize();
    if (point.x() < 0 || point.y() < 0 || point.x() >= grid.width() || point.y() >= grid.height())
        return false;
    auto encoded = m_registry->encodeMouse(
        surfaceIdentity(),
        {
            .action = action,
            .button = button,
            .modifiers = modifiers(keyboardModifiers),
            .x = static_cast<float>(point.x()),
            .y = static_cast<float>(point.y()),
            .screenWidth = static_cast<std::uint32_t>(
                std::ceil(grid.width())),
            .screenHeight = static_cast<std::uint32_t>(
                std::ceil(grid.height())),
            .cellWidth = static_cast<std::uint32_t>(
                std::ceil(cellSize.width())),
            .cellHeight = static_cast<std::uint32_t>(
                std::ceil(cellSize.height())),
            .anyButtonPressed = anyButtonPressed,
        });
    if (!encoded) {
        emit operationError(encoded.error().message);
        return true;
    }
    if (encoded->isEmpty()) {
        return false;
    }
    (void)enqueueInput(std::move(*encoded));
    return true;
}

void TerminalView::sendText(const QString& text)
{
    if (!interactionAvailable() || !m_canSendInput || m_runtime == nullptr || text.isEmpty()) {
        return;
    }
    (void)enqueueInput(text.toUtf8());
}

void TerminalView::pasteClipboard()
{
    if (!interactionAvailable() || !m_canSendInput || m_registry == nullptr || m_runtime == nullptr
        || !m_terminalReady) {
        return;
    }
    auto* clipboard = QGuiApplication::clipboard();
    if (clipboard == nullptr) {
        return;
    }
    auto text = clipboard->text(QClipboard::Clipboard).toUtf8();
    if (text.isEmpty()) {
        return;
    }
    if (text.size() > maximumQueuedInputBytes
        || m_queuedInputBytes > maximumQueuedInputBytes - text.size()
        || m_inputQueue.size() >= maximumQueuedInputFrames) {
        emit operationError(QStringLiteral("Terminal input queue is full; input was not accepted."));
        return;
    }
    auto encoded = m_registry->encodePaste(surfaceIdentity(), std::move(text));
    if (!encoded) {
        emit operationError(encoded.error().message);
        return;
    }
    (void)enqueueInput(std::move(*encoded));
}

bool TerminalView::sendKey(QKeyEvent* event, const TerminalKeyAction action)
{
    const auto key = terminalKey(event);
    if (!interactionAvailable() || !m_canSendInput || !key || m_registry == nullptr
        || m_runtime == nullptr) {
        return false;
    }
    auto text = event->text();
    const auto unshifted = unshiftedCodepoint(event->key());
    if (unshifted != 0 && !text.isEmpty()) {
        const auto hasControlText = text.size() != 1
            || text.front().unicode() < QLatin1Char(' ').unicode()
            || text.front().unicode() == 0x7f;
        if (hasControlText) {
            auto character = QChar(static_cast<char16_t>(unshifted));
            if (event->modifiers().testFlag(Qt::ShiftModifier)) {
                character = character.toUpper();
            }
            text = QString(character);
        }
    } else if (*key != TerminalKey::Character && *key != TerminalKey::Digit
        && *key != TerminalKey::Backquote && *key != TerminalKey::Backslash
        && *key != TerminalKey::BracketLeft && *key != TerminalKey::BracketRight
        && *key != TerminalKey::Comma && *key != TerminalKey::Equal
        && *key != TerminalKey::Minus && *key != TerminalKey::Period
        && *key != TerminalKey::Quote && *key != TerminalKey::Semicolon
        && *key != TerminalKey::Slash && *key != TerminalKey::Space) {
        text.clear();
    }
    auto encoded = m_registry->encodeKey(
        surfaceIdentity(),
        {
            .key = *key,
            .action = action,
            .text = std::move(text),
            .unshiftedCodepoint = unshifted,
            .modifiers = modifiers(event->modifiers()),
        });
    if (!encoded) {
        emit operationError(encoded.error().message);
        return true;
    }
    (void)enqueueInput(std::move(*encoded));
    return true;
}

bool TerminalView::enqueueInput(QByteArray bytes)
{
    if (!interactionAvailable() || !m_canSendInput || bytes.isEmpty() || m_registry == nullptr
        || m_runtime == nullptr || !m_terminalReady) {
        return false;
    }
    if (bytes.size() > maximumQueuedInputBytes
        || m_queuedInputBytes > maximumQueuedInputBytes - bytes.size()
        || m_inputQueue.size() >= maximumQueuedInputFrames) {
        emit operationError(QStringLiteral("Terminal input queue is full; input was not accepted."));
        return false;
    }
    auto bottom = m_registry->scrollViewportToBottom(surfaceIdentity());
    if (!bottom) {
        emit operationError(bottom.error().message);
        return false;
    }
    m_queuedInputBytes += bytes.size();
    m_inputQueue.push_back(std::move(bytes));
    drainInputQueue();
    return true;
}

void TerminalView::drainInputQueue()
{
    if (!interactionAvailable() || !m_canSendInput || m_runtime == nullptr
        || m_inputRetryQueued) {
        return;
    }
    for (int sent = 0; sent < 64 && !m_inputQueue.empty(); ++sent) {
        auto result = m_runtime->sendTerminalInput(
            m_subscription,
            m_expectedRuntimeIncarnationId,
            m_inputQueue.front());
        if (result) {
            m_queuedInputBytes -= m_inputQueue.front().size();
            m_inputQueue.pop_front();
            m_inputBackoffStep = 0;
            continue;
        }
        if (result.error().ffiResult == KODOSI_FFI_BUSY) {
            const auto step = std::min<std::size_t>(
                m_inputBackoffStep,
                std::size(inputBackoffMilliseconds) - 1);
            const auto epoch = m_attachmentEpoch.load(std::memory_order_acquire);
            const auto inputGeneration = m_inputGeneration;
            m_inputRetryQueued = true;
            m_inputBackoffStep = static_cast<std::uint8_t>(
                std::min<std::size_t>(
                    step + 1,
                    std::size(inputBackoffMilliseconds) - 1));
            QTimer::singleShot(
                inputBackoffMilliseconds[step],
                this,
                [this, epoch, inputGeneration] {
                    if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch
                        || m_inputGeneration != inputGeneration) {
                        return;
                    }
                    m_inputRetryQueued = false;
                    drainInputQueue();
                });
            return;
        }

        const auto message = result.error().message;
        m_inputQueue.clear();
        m_queuedInputBytes = 0;
        m_inputBackoffStep = 0;
        emit operationError(message);
        return;
    }
    if (!m_inputQueue.empty()) {
        const auto epoch = m_attachmentEpoch.load(std::memory_order_acquire);
        const auto inputGeneration = m_inputGeneration;
        m_inputRetryQueued = true;
        QTimer::singleShot(0, this, [this, epoch, inputGeneration] {
            if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch
                || m_inputGeneration != inputGeneration) {
                return;
            }
            m_inputRetryQueued = false;
            drainInputQueue();
        });
    }
}

std::optional<QPoint> TerminalView::terminalCellAt(
    const QPointF& position,
    const CellHitTest hitTest) const
{
    if (!m_frame || m_frame->columns == 0 || m_frame->rows == 0) {
        return std::nullopt;
    }
    const auto cell = TerminalRasterizer::cellSize(m_font, m_lineHeight);
    if (cell.width() <= 0.0 || cell.height() <= 0.0) {
        return std::nullopt;
    }
    const auto point = gridPoint(position);
    if (hitTest == CellHitTest::Strict
        && (position.x() < 0.0 || position.y() < 0.0
            || position.x() >= width() || position.y() >= height()
            || point.x() < 0.0 || point.y() < 0.0
            || point.x() >= cell.width() * m_frame->columns
            || point.y() >= cell.height() * m_frame->rows)) {
        return std::nullopt;
    }
    const auto column = std::clamp(
        static_cast<int>(std::floor(point.x() / cell.width())),
        0,
        static_cast<int>(m_frame->columns) - 1);
    const auto row = std::clamp(
        static_cast<int>(std::floor(point.y() / cell.height())),
        0,
        static_cast<int>(m_frame->rows) - 1);
    return QPoint(column, row);
}

std::optional<QUrl> TerminalView::linkAtCell(
    const QPoint& cell,
    const bool reportFailure)
{
    if (m_registry == nullptr || !m_terminalReady) {
        return std::nullopt;
    }
    auto value = m_registry->linkAt(
        surfaceIdentity(),
        m_frame->viewportRevision,
        static_cast<std::uint16_t>(cell.x()),
        static_cast<std::uint16_t>(cell.y()));
    if (!value) {
        if (reportFailure
            && value.error().code
                != GhosttyTerminalKernel::Failure::Code::StaleFrame) {
            emit operationError(value.error().message);
        }
        return std::nullopt;
    }
    return validatedTerminalLink(*value);
}

void TerminalView::updateHoverLink(const QPointF& position)
{
    const auto cell = terminalCellAt(position, CellHitTest::Strict);
    if (cell && linkAtCell(*cell, false)) {
        setCursor(Qt::PointingHandCursor);
    } else {
        unsetCursor();
    }
}

void TerminalView::clearHoverLink()
{
    unsetCursor();
}

int TerminalView::wheelRows(QWheelEvent* event)
{
    const auto cell = TerminalRasterizer::cellSize(m_font, m_lineHeight);
    if (event->pixelDelta().y() != 0 && cell.height() > 0.0) {
        const auto bounded = std::clamp(
            static_cast<qreal>(event->pixelDelta().y()),
            -cell.height() * maximumWheelRowsPerEvent,
            cell.height() * maximumWheelRowsPerEvent);
        m_wheelPixelRemainder += bounded;
        const auto rows = std::clamp(
            static_cast<int>(m_wheelPixelRemainder / cell.height()),
            -maximumWheelRowsPerEvent,
            maximumWheelRowsPerEvent);
        m_wheelPixelRemainder -= rows * cell.height();
        return -rows;
    }
    if (event->angleDelta().y() != 0) {
        const auto bounded = std::clamp(
            event->angleDelta().y(),
            -120 * maximumWheelRowsPerEvent,
            120 * maximumWheelRowsPerEvent);
        m_wheelAngleRemainder += bounded;
        const auto rows = std::clamp(
            m_wheelAngleRemainder / 120,
            -maximumWheelRowsPerEvent,
            maximumWheelRowsPerEvent);
        m_wheelAngleRemainder -= rows * 120;
        return -rows;
    }
    return 0;
}

void TerminalView::beginSelection(const QPoint& anchor)
{
    if (m_registry == nullptr || m_frame == nullptr) {
        return;
    }
    auto selected = m_registry->beginSelection(
        surfaceIdentity(),
        m_frame->viewportRevision,
        static_cast<std::uint16_t>(anchor.x()),
        static_cast<std::uint16_t>(anchor.y()));
    m_selecting = selected.has_value();
    if (!selected && !selected.error().isRecoverableHitTestRace()) {
        emit operationError(selected.error().message);
    }
}

void TerminalView::updateSelection(const QPoint& endpoint)
{
    if (m_registry == nullptr || m_frame == nullptr) {
        return;
    }
    auto selected = m_registry->updateSelection(
        surfaceIdentity(),
        m_frame->viewportRevision,
        static_cast<std::uint16_t>(endpoint.x()),
        static_cast<std::uint16_t>(endpoint.y()));
    if (!selected) {
        if (selected.error().isRecoverableHitTestRace()) {
            m_selecting = false;
            if (auto cleared = m_registry->clearSelection(surfaceIdentity());
                !cleared) {
                emit operationError(cleared.error().message);
            }
            return;
        }
        emit operationError(selected.error().message);
    }
}

bool TerminalView::copySelection()
{
    if (m_registry == nullptr) {
        return false;
    }
    auto text = m_registry->selectedText(surfaceIdentity());
    if (!text) {
        emit operationError(text.error().message);
        return true;
    }
    if (text->isEmpty()) {
        return false;
    }
    QGuiApplication::clipboard()->setText(*text, QClipboard::Clipboard);
    return true;
}

}
