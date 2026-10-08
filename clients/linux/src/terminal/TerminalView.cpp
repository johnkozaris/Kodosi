#include "terminal/TerminalView.hpp"
#include "runtime/JsonEnvelope.hpp"
#include "logging/ApplicationLogStore.hpp"

#include <kodosi_runtime.h>

#include <QAccessible>
#include <QFontDatabase>
#include <QFocusEvent>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMetaObject>
#include <QQuickWindow>
#include <QRawFont>
#include <QThread>
#include <QTimer>
#include <QUuid>
#include <QtQuick/private/qquickaccessibleattached_p.h>

#include <algorithm>
#include <cmath>
#include <iterator>
#include <limits>
#include <optional>
#include <utility>

namespace kodosi {
namespace {

constexpr char32_t powerlineRightSeparator = 0xE0B0;
constexpr char32_t powerlineLeftCap = 0xE0B6;
constexpr char32_t nerdFontLinux = 0xF31B;

bool supportsTerminalSymbols(const QString& family)
{
    auto candidate = QFont(family);
    candidate.setStyleStrategy(QFont::NoFontMerging);
    const auto rawFont = QRawFont::fromFont(candidate);
    return rawFont.isValid()
        && rawFont.supportsCharacter(powerlineRightSeparator)
        && rawFont.supportsCharacter(powerlineLeftCap)
        && rawFont.supportsCharacter(nerdFontLinux);
}

QString terminalSymbolFamily()
{
    static const auto family = [] {
        const QFontDatabase database;
        const QStringList preferred {
            QStringLiteral("Symbols Nerd Font Mono"),
            QStringLiteral("JetBrainsMono Nerd Font Mono"),
            QStringLiteral("MesloLGLDZ Nerd Font Mono"),
        };
        const auto installed = database.families();
        for (const auto& candidate : preferred) {
            if (installed.contains(candidate)
                && supportsTerminalSymbols(candidate)) {
                return candidate;
            }
        }
        for (const auto& candidate : installed) {
            if (database.isFixedPitch(candidate)
                && supportsTerminalSymbols(candidate)) {
                return candidate;
            }
        }
        return QString {};
    }();
    return family;
}

void configureTerminalFont(QFont& font, const QString& requestedFamily)
{
    QStringList families {requestedFamily};
    const auto symbolFamily = terminalSymbolFamily();
    if (!symbolFamily.isEmpty() && symbolFamily != requestedFamily) {
        families.push_back(symbolFamily);
    }
    font.setFamilies(families);
    font.setFixedPitch(true);
    font.setStyleHint(QFont::Monospace);
}

void publishAccessibleTextChange(
    QObject* object,
    const QString& previous,
    const QString& current)
{
    if (previous == current) {
        return;
    }
    qsizetype prefix = 0;
    const auto shared = std::min(previous.size(), current.size());
    while (prefix < shared && previous[prefix] == current[prefix]) {
        ++prefix;
    }
    if (prefix > 0 && prefix < previous.size()
        && previous[prefix - 1].isHighSurrogate()
        && previous[prefix].isLowSurrogate()) {
        --prefix;
    }

    qsizetype suffix = 0;
    while (suffix < previous.size() - prefix
        && suffix < current.size() - prefix
        && previous[previous.size() - suffix - 1]
            == current[current.size() - suffix - 1]) {
        ++suffix;
    }
    const auto previousEnd = previous.size() - suffix;
    const auto currentEnd = current.size() - suffix;
    if (suffix > 0
        && ((previousEnd > 0 && previousEnd < previous.size()
                && previous[previousEnd - 1].isHighSurrogate()
                && previous[previousEnd].isLowSurrogate())
            || (currentEnd > 0 && currentEnd < current.size()
                && current[currentEnd - 1].isHighSurrogate()
                && current[currentEnd].isLowSurrogate()))) {
        --suffix;
    }

    QAccessibleTextUpdateEvent event(
        object,
        static_cast<int>(prefix),
        previous.sliced(prefix, previous.size() - prefix - suffix),
        current.sliced(prefix, current.size() - prefix - suffix));
    QAccessible::updateAccessibility(&event);
}

}

void detail::TerminalFrameMailbox::reset(const std::uint64_t incarnation)
{
    std::scoped_lock lock(m_mutex);
    m_pendingFrame.reset();
    m_incarnation = incarnation;
    m_highestDisplayRevision = 0;
    m_drainQueued = false;
}

detail::TerminalFrameMailbox::EnqueueResult detail::TerminalFrameMailbox::enqueue(
    const std::uint64_t incarnation,
    GhosttyTerminalKernel::Frame frame)
{
    std::scoped_lock lock(m_mutex);
    if (incarnation != m_incarnation || frame == nullptr
        || frame->displayRevision <= m_highestDisplayRevision) {
        return {};
    }
    m_highestDisplayRevision = frame->displayRevision;
    m_pendingFrame = std::move(frame);
    const auto queueDrain = !m_drainQueued;
    m_drainQueued = true;
    return {
        .accepted = true,
        .queueDrain = queueDrain,
    };
}

GhosttyTerminalKernel::Frame detail::TerminalFrameMailbox::take(
    const std::uint64_t incarnation)
{
    std::scoped_lock lock(m_mutex);
    if (incarnation != m_incarnation) {
        return {};
    }
    auto frame = std::exchange(m_pendingFrame, {});
    m_drainQueued = false;
    return frame;
}

TerminalView::TerminalView(QQuickItem* parent)
    : QQuickItem(parent)
    , m_font(QFontDatabase::systemFont(QFontDatabase::FixedFont))
    , m_cursorBlinkTimer(this)
{
    setFlag(ItemHasContents, true);
    setClip(true);
    setFlag(ItemAcceptsInputMethod, true);
    setActiveFocusOnTab(true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setAcceptHoverEvents(true);
    auto* accessible = qobject_cast<QQuickAccessibleAttached*>(
        qmlAttachedPropertiesObject<QQuickAccessibleAttached>(this, true));
    accessible->setRole(QAccessible::Terminal);
    accessible->setName(tr("Terminal"));
    accessible->set_focusable(true);
    accessible->set_editable(false);
    accessible->set_readOnly(true);
    accessible->set_multiLine(true);
    accessible->set_selectableText(true);
    configureTerminalFont(m_font, QStringLiteral("JetBrains Mono"));
    m_font.setPixelSize(14);
    m_cursorBlinkTimer.setObjectName(QStringLiteral("terminal.cursorBlinkTimer"));
    m_cursorBlinkTimer.setInterval(500);
    connect(&m_cursorBlinkTimer, &QTimer::timeout, this, [this] {
        if (!blinkEligible()) {
            updateBlinkTimer();
            return;
        }
        m_cursorPhaseVisible = !m_cursorPhaseVisible;
        m_renderedFrame.reset();
        update();
    });
    connect(this, &QQuickItem::enabledChanged, this, &TerminalView::synchronizeInteraction);
    connect(this, &QQuickItem::opacityChanged, this, &TerminalView::synchronizeInteraction);
}

TerminalView::~TerminalView()
{
    m_renderPerformanceGeneration.fetch_add(1, std::memory_order_acq_rel);
    detach();
}

QSizeF TerminalView::gridSize() const
{
    return m_frame ? TerminalRasterizer::logicalSize(*m_frame, m_font, m_lineHeight) : QSizeF {};
}

qreal TerminalView::viewportScale() const
{
    const auto grid = gridSize();
    if (grid.isEmpty() || width() <= 0 || height() <= 0)
        return 1.0;
    return std::min({1.0, width() / grid.width(), height() / grid.height()});
}

QPointF TerminalView::viewportOffset() const
{
    return {};
}

QPointF TerminalView::gridPoint(const QPointF& point) const
{
    return (point - viewportOffset()) / viewportScale();
}

void TerminalView::updateViewport()
{
    emit viewportChanged();
    update();
}

QString TerminalView::fontFamily() const
{
    return m_font.family();
}

int TerminalView::fontPixelSize() const noexcept
{
    return m_font.pixelSize();
}

double TerminalView::lineHeight() const noexcept
{
    return m_lineHeight;
}

int TerminalView::cursorStyle() const noexcept
{
    switch (m_cursorStyle) {
    case TerminalCursorStyle::Block:
        return 0;
    case TerminalCursorStyle::Bar:
        return 1;
    case TerminalCursorStyle::Underline:
        return 2;
    case TerminalCursorStyle::HollowBlock:
        return 0;
    }
    return 0;
}

bool TerminalView::cursorBlink() const noexcept
{
    return m_cursorBlink;
}

int TerminalView::scrollbackLines() const noexcept
{
    return m_scrollbackLines;
}

QColor TerminalView::selectionBackground() const
{
    return m_palette.selectionBackground;
}

QColor TerminalView::selectionForeground() const
{
    return m_palette.selectionForeground;
}

QColor TerminalView::preeditBackground() const
{
    return m_palette.preeditBackground;
}

QColor TerminalView::preeditForeground() const
{
    return m_palette.preeditForeground;
}

bool TerminalView::terminalReady() const noexcept
{
    return m_terminalReady;
}

bool TerminalView::canSendInput() const noexcept
{
    return m_canSendInput;
}

bool TerminalView::canResize() const noexcept
{
    return m_canResize;
}

bool TerminalView::readOnly() const noexcept
{
    return !m_canSendInput;
}

bool TerminalView::hasSelection() const
{
    const auto selection = accessibleSelection();
    return selection.first >= 0 && selection.second > selection.first;
}

bool TerminalView::copySelectionToClipboard()
{
    return copySelection();
}

void TerminalView::pasteFromClipboard()
{
    pasteClipboard();
}

bool TerminalView::focusedSizeAuthority() const noexcept
{
    return m_focusedSizeAuthority;
}

void TerminalView::setFontFamily(const QString& family)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto trimmed = family.trimmed();
    if (trimmed.isEmpty() || trimmed == m_font.family()) {
        return;
    }
    configureTerminalFont(m_font, trimmed);
    invalidateMetrics();
    emit fontFamilyChanged();
}

void TerminalView::setFontPixelSize(const int size)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto bounded = std::clamp(size, 8, 32);
    if (bounded == m_font.pixelSize()) {
        return;
    }
    m_font.setPixelSize(bounded);
    invalidateMetrics();
    emit fontPixelSizeChanged();
}

void TerminalView::setLineHeight(const double lineHeight)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto bounded = std::clamp(lineHeight, 0.8, 2.0);
    if (qFuzzyCompare(bounded, m_lineHeight)) {
        return;
    }
    m_lineHeight = bounded;
    invalidateMetrics();
    emit lineHeightChanged();
}

void TerminalView::setCursorStyle(const int style)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto next = style == 1
        ? TerminalCursorStyle::Bar
        : style == 2 ? TerminalCursorStyle::Underline : TerminalCursorStyle::Block;
    if (next == m_cursorStyle) {
        return;
    }
    m_cursorStyle = next;
    m_renderedFrame.reset();
    emit cursorStyleChanged();
    update();
    configureAttachedKernel();
}

void TerminalView::setCursorBlink(const bool enabled)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (enabled == m_cursorBlink) {
        return;
    }
    m_cursorBlink = enabled;
    m_renderedFrame.reset();
    emit cursorBlinkChanged();
    updateBlinkTimer();
    update();
    configureAttachedKernel();
}

void TerminalView::setScrollbackLines(const int lines)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto bounded = std::clamp(lines, 100, 100'000);
    if (bounded == m_scrollbackLines) {
        return;
    }
    m_scrollbackLines = bounded;
    emit scrollbackLinesChanged();
    configureAttachedKernel();
}

void TerminalView::setSelectionBackground(const QColor& color)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (!color.isValid() || color == m_palette.selectionBackground) {
        return;
    }
    m_palette.selectionBackground = color;
    m_renderedFrame.reset();
    emit selectionBackgroundChanged();
    update();
}

void TerminalView::setSelectionForeground(const QColor& color)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (!color.isValid() || color == m_palette.selectionForeground) {
        return;
    }
    m_palette.selectionForeground = color;
    m_renderedFrame.reset();
    emit selectionForegroundChanged();
    update();
}

void TerminalView::setPreeditBackground(const QColor& color)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (!color.isValid() || color == m_palette.preeditBackground) {
        return;
    }
    m_palette.preeditBackground = color;
    m_renderedFrame.reset();
    emit preeditBackgroundChanged();
    update();
}

void TerminalView::setPreeditForeground(const QColor& color)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (!color.isValid() || color == m_palette.preeditForeground) {
        return;
    }
    m_palette.preeditForeground = color;
    m_renderedFrame.reset();
    emit preeditForegroundChanged();
    update();
}

void TerminalView::setTerminalInteraction(
    const bool canSendInput,
    const bool canResize)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_canSendInput == canSendInput
        && m_canResize == canResize) {
        return;
    }
    const auto wasReadOnly = readOnly();
    const auto desiredFocus = m_desiredFocus;
    const auto releaseFocus =
        (m_canSendInput && !canSendInput)
        && (m_focusClaimed || !m_pendingFocusRequestId.isEmpty()
            || desiredFocus);
    const auto gainedFocus =
        !m_canSendInput && canSendInput && desiredFocus;
    const auto gainedResize =
        !m_canResize && canResize;
    m_canSendInput = canSendInput;
    m_canResize = canResize;
    if (!m_canSendInput) {
        clearPendingInput();
        m_preedit.clear();
        m_renderedPreedit.clear();
        m_pasteShortcutKey.reset();
        if (auto* inputMethod = QGuiApplication::inputMethod()) {
            inputMethod->reset();
            inputMethod->update(Qt::ImEnabled | Qt::ImCursorRectangle);
        }
    }
    if (releaseFocus) {
        sendFocus(false);
        m_desiredFocus = desiredFocus;
    }
    if (gainedFocus && m_desiredFocus
        && !(m_focusRetryQueued
            && m_focusRetryOperation == FocusOperation::Blur)) {
        sendFocus(true);
    }
    if (!m_canResize) {
        m_resizeRetryQueued = false;
        m_pendingResize.reset();
        m_resizeClaimPending = false;
        m_lastResizeKey.clear();
    } else if (gainedResize) {
        m_resizeClaimPending = m_focusedSizeAuthority;
        scheduleResize();
    }
    emit capabilitiesChanged();
    updateViewport();
    if (wasReadOnly != readOnly()) {
        if (auto* accessible = qobject_cast<QQuickAccessibleAttached*>(
                qmlAttachedPropertiesObject<QQuickAccessibleAttached>(
                    this,
                    true))) {
            accessible->set_readOnly(readOnly());
            accessible->set_editable(!readOnly());
        }
        if (QAccessible::isActive()) {
            QAccessible::State changed;
            changed.readOnly = true;
            changed.editable = true;
            QAccessibleStateChangeEvent event(this, changed);
            QAccessible::updateAccessibility(&event);
        }
    }
    update();
}

void TerminalView::setFocusedSizeAuthority(const bool focused)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_focusedSizeAuthority == focused) {
        return;
    }
    m_focusedSizeAuthority = focused;
    m_resizeClaimPending = focused && m_canResize;
    emit focusedSizeAuthorityChanged();
    if (m_resizeClaimPending) {
        scheduleResize();
    }
}

TerminalSurfaceIdentity TerminalView::surfaceIdentity() const
{
    return {
        .subscription = m_subscription,
        .surfaceGeneration = m_surfaceGeneration,
    };
}

bool TerminalView::attach(
    TerminalSessionRegistry& registry,
    TerminalCommandDispatcher& runtime,
    TerminalSubscription subscription,
    QString expectedRuntimeIncarnationId,
    const std::uint64_t surfaceGeneration)
{
    Q_ASSERT(thread() == QThread::currentThread());
    detach();
    if (subscription.sessionId.isEmpty() || subscription.subscriptionId.isEmpty()
        || expectedRuntimeIncarnationId.isEmpty() || surfaceGeneration == 0) {
        return false;
    }

    m_registry = &registry;
    m_runtime = &runtime;
    m_subscription = std::move(subscription);
    m_expectedRuntimeIncarnationId = std::move(expectedRuntimeIncarnationId);
    m_surfaceGeneration = surfaceGeneration;
    m_renderPerformanceGeneration.fetch_add(1, std::memory_order_acq_rel);
    const auto epoch = m_attachmentEpoch.fetch_add(1, std::memory_order_acq_rel) + 1;
    m_frameMailbox.reset(epoch);
    const auto registered = m_registry->registerSurface(
        surfaceIdentity(),
        {
            .frameChanged = [this, epoch](auto frame) {
                enqueueFrame(epoch, std::move(frame));
            },
            .failed = [this, epoch](auto failure) {
                failure.message = failure.message.left(1024);
                enqueueGuiEvent(epoch, std::move(failure));
            },
            .connectionCompleted = [this, epoch](std::int32_t result) {
                enqueueGuiEvent(epoch, result);
            },
            .focusCompleted = [this, epoch](auto outcome) {
                outcome.reason = outcome.reason.left(1024);
                enqueueGuiEvent(epoch, std::move(outcome));
            },
            .resizeCompleted = [this, epoch](auto outcome) {
                outcome.reason = outcome.reason.left(1024);
                enqueueGuiEvent(epoch, std::move(outcome));
            },
            .closed = [this, epoch] { enqueueGuiEvent(epoch, Closed {}); },
        },
        kernelSettings());
    if (!registered) {
        m_registry = nullptr;
        m_runtime = nullptr;
        return false;
    }

    auto connected = registered->requiresConnection
        ? m_runtime->connectTerminal(m_subscription)
        : registered->requiresRefresh
        ? m_runtime->refreshTerminal(m_subscription)
        : TerminalCommandDispatcher::Result {};
    if (!connected) {
        const auto message = connected.error().message;
        if (registered->requiresRefresh) {
            m_registry->cancelSurfaceRefresh(surfaceIdentity());
        }
        (void)m_registry->unregisterSurface(surfaceIdentity());
        m_registry = nullptr;
        m_runtime = nullptr;
        emit terminalError(message);
        return false;
    }
    m_resizeClaimPending =
        m_focusedSizeAuthority && m_canResize;
    scheduleResize();
    return true;
}

void TerminalView::detach()
{
    m_renderPerformanceGeneration.fetch_add(1, std::memory_order_acq_rel);
    Q_ASSERT(thread() == QThread::currentThread());
    const auto priorAccessibleText = QAccessible::isActive() ? accessibleText() : QString {};
    const auto wasReady = m_terminalReady;
    const auto ownsComposition = hasActiveFocus() && !m_preedit.isEmpty();
    if (m_runtime != nullptr && (m_focusClaimed || !m_pendingFocusRequestId.isEmpty())) {
        sendFocus(false);
        for (const auto delay : inputBackoffMilliseconds) {
            if (!m_focusRetryQueued) {
                break;
            }
            m_focusRetryQueued = false;
            m_focusRetryOperation = FocusOperation::None;
            QThread::msleep(static_cast<unsigned long>(delay));
            sendFocus(false);
        }
    }
    const auto epoch = m_attachmentEpoch.fetch_add(1, std::memory_order_acq_rel) + 1;
    m_frameMailbox.reset(epoch);
    {
        std::scoped_lock lock(m_guiEventMutex);
        m_guiEvents.clear();
        m_guiDrainQueued = false;
        m_guiOverflow = false;
    }
    auto* registry = std::exchange(m_registry, nullptr);
    auto* runtime = std::exchange(m_runtime, nullptr);
    bool disconnectSubscription = false;
    if (registry != nullptr) {
        disconnectSubscription = registry->unregisterSurface(surfaceIdentity());
    }
    if (disconnectSubscription && runtime != nullptr && runtime->isRunning()) {
        (void)runtime->disconnectTerminal(m_subscription);
    }
    m_subscription = {};
    m_expectedRuntimeIncarnationId.clear();
    m_surfaceGeneration = 0;
    m_resizeQueued = false;
    m_lastResizeKey.clear();
    m_pendingResize.reset();
    m_pendingFocusRequestId.clear();
    m_focusClaimed = false;
    m_terminalReady = false;
    m_focusRetryQueued = false;
    m_focusRetryOperation = FocusOperation::None;
    m_resizeRetryQueued = false;
    m_resizeClaimPending = false;
    m_desiredFocus = false;
    clearPendingInput();
    m_preedit.clear();
    m_renderedPreedit.clear();
    m_selecting = false;
    m_reportedMouseButton = Qt::NoButton;
    m_copyShortcutActive = false;
    m_pasteShortcutKey.reset();
    m_pressedLink.reset();
    m_wheelAngleRemainder = 0;
    m_wheelPixelRemainder = 0.0;
    clearHoverLink();
    updateBlinkTimer();
    if (auto* inputMethod = QGuiApplication::inputMethod()) {
        if (ownsComposition) {
            inputMethod->reset();
        }
        inputMethod->update(Qt::ImEnabled | Qt::ImCursorRectangle);
    }
    m_frame.reset();
    m_renderedFrame.reset();
    updateViewport();
    if (wasReady) {
        emit terminalReadyChanged();
    }
    if (QAccessible::isActive()) {
        publishAccessibleTextChange(this, priorAccessibleText, {});
        if (wasReady) {
            QAccessible::State changed;
            changed.disabled = true;
            QAccessibleStateChangeEvent event(this, changed);
            QAccessible::updateAccessibility(&event);
        }
    }
    update();
}

void TerminalView::geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry)
{
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) {
        updateViewport();
        scheduleResize();
    }
}

void TerminalView::focusInEvent(QFocusEvent* event)
{
    QQuickItem::focusInEvent(event);
    sendFocus(true);
    updateBlinkTimer();
}

void TerminalView::focusOutEvent(QFocusEvent* event)
{
    sendFocus(false);
    m_copyShortcutActive = false;
    m_pasteShortcutKey.reset();
    m_preedit.clear();
    updateBlinkTimer();
    update();
    QQuickItem::focusOutEvent(event);
}

void TerminalView::itemChange(const ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged) {
        update();
        scheduleResize();
    } else if (change == ItemVisibleHasChanged) {
        synchronizeInteraction();
        updateBlinkTimer();
        scheduleResize();
    } else if (change == ItemSceneChange) {
        QObject::disconnect(m_windowVisibilityConnection);
        if (value.window != nullptr) {
            m_windowVisibilityConnection = connect(
                value.window,
                &QWindow::visibilityChanged,
                this,
                [this] {
                    synchronizeInteraction();
                    updateBlinkTimer();
                    scheduleResize();
                });
        }
        synchronizeInteraction();
        updateBlinkTimer();
    }
}

bool TerminalView::interactionAvailable() const
{
    return isVisible() && isEnabled() && opacity() > 0 && window() != nullptr
        && window()->isVisible() && window()->visibility() != QWindow::Minimized;
}

void TerminalView::clearPendingInput()
{
    ++m_inputGeneration;
    m_inputQueue.clear();
    m_queuedInputBytes = 0;
    m_inputRetryQueued = false;
    m_inputBackoffStep = 0;
}

void TerminalView::synchronizeInteraction()
{
    if (interactionAvailable()) {
        if (auto* inputMethod = QGuiApplication::inputMethod())
            inputMethod->update(Qt::ImEnabled | Qt::ImCursorRectangle);
        if (hasActiveFocus() && m_terminalReady) {
            m_desiredFocus = true;
            if (!(m_focusRetryQueued && m_focusRetryOperation == FocusOperation::Blur))
                sendFocus(true);
        }
        return;
    }
    clearPendingInput();
    const bool ownsComposition = hasActiveFocus() && !m_preedit.isEmpty();
    m_preedit.clear();
    m_renderedPreedit.clear();
    m_pasteShortcutKey.reset();
    m_copyShortcutActive = false;
    m_pressedLink.reset();
    m_selecting = false;
    m_reportedMouseButton = Qt::NoButton;
    if (m_focusClaimed || !m_pendingFocusRequestId.isEmpty() || m_desiredFocus)
        sendFocus(false);
    m_desiredFocus = false;
    if (auto* inputMethod = QGuiApplication::inputMethod()) {
        if (ownsComposition) inputMethod->reset();
        inputMethod->update(Qt::ImEnabled | Qt::ImCursorRectangle);
    }
    update();
}

void TerminalView::enqueueGuiEvent(const std::uint64_t epoch, GuiEvent event)
{
    std::scoped_lock lock(m_guiEventMutex);
    if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch || m_guiOverflow)
        return;
    if (m_guiEvents.size() >= 64) {
        m_guiEvents.clear();
        m_guiOverflow = true;
    } else {
        m_guiEvents.push_back(std::move(event));
    }
    if (!m_guiDrainQueued) {
        m_guiDrainQueued = true;
        QMetaObject::invokeMethod(this, [this, epoch] { drainGuiEvents(epoch); }, Qt::QueuedConnection);
    }
}

void TerminalView::drainGuiEvents(const std::uint64_t epoch)
{
    std::deque<GuiEvent> events;
    bool overflow = false;
    {
        std::scoped_lock lock(m_guiEventMutex);
        if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) return;
        events.swap(m_guiEvents);
        overflow = m_guiOverflow;
        m_guiDrainQueued = false;
    }
    if (overflow) {
        detach();
        emit terminalError(tr("Terminal updates exceeded their limit. Reconnect this view."));
        return;
    }
    for (auto& event : events) {
        if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) return;
        std::visit([this, epoch](auto&& value) {
            using T = std::decay_t<decltype(value)>;
            if constexpr (std::is_same_v<T, GhosttyTerminalKernel::Failure>) {
                presentFailure(std::move(value));
            } else if constexpr (std::is_same_v<T, std::int32_t>) {
                emit connectionCompleted(value == KODOSI_FFI_OK, value);
                if (m_attachmentEpoch.load(std::memory_order_acquire) == epoch
                    && value == KODOSI_FFI_OK && hasActiveFocus() && interactionAvailable()) sendFocus(true);
            } else if constexpr (std::is_same_v<T, TerminalFocusOutcome>) {
                handleFocusOutcome(std::move(value));
            } else if constexpr (std::is_same_v<T, TerminalResizeOutcome>) {
                handleResizeOutcome(std::move(value));

            } else {
                drainFrame(epoch);
                if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) return;
                detach();
                emit terminalClosed();
            }
        }, std::move(event));
    }
}

void TerminalView::enqueueFrame(
    const std::uint64_t epoch,
    GhosttyTerminalKernel::Frame frame)
{
    if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) {
        return;
    }
    const auto enqueued = m_frameMailbox.enqueue(epoch, std::move(frame));
    if (enqueued.queueDrain) {
        QMetaObject::invokeMethod(
            this,
            [this, epoch] { drainFrame(epoch); },
            Qt::QueuedConnection);
    }
}

void TerminalView::drainFrame(const std::uint64_t epoch)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) {
        return;
    }
    auto frame = m_frameMailbox.take(epoch);
    if (frame) {
        presentFrame(std::move(frame));
    }
}

void TerminalView::presentFrame(GhosttyTerminalKernel::Frame frame)
{
    Q_ASSERT(thread() == QThread::currentThread());
    const auto priorAccessibleText = QAccessible::isActive() ? accessibleText() : QString {};
    m_frame = std::move(frame);
    m_pressedLink.reset();
    clearHoverLink();
    const auto wasReady = m_terminalReady;
    m_terminalReady = m_frame != nullptr;
    if (m_frame) {
        const auto sizeHint =
            TerminalRasterizer::logicalSize(*m_frame, m_font, m_lineHeight);
        setImplicitSize(sizeHint.width(), sizeHint.height());
    }
    emit frameChanged();
    updateViewport();
    if (wasReady != m_terminalReady) {
        emit terminalReadyChanged();
    }
    update();
    updateBlinkTimer();
    scheduleResize();
    if (auto* inputMethod = QGuiApplication::inputMethod()) {
        Qt::InputMethodQueries queries = Qt::ImCursorRectangle;
        if (wasReady != m_terminalReady) {
            queries |= Qt::ImEnabled;
        }
        inputMethod->update(queries);
    }
    if (QAccessible::isActive()) {
        publishAccessibleTextChange(this, priorAccessibleText, accessibleText());
        if (wasReady != m_terminalReady) {
            QAccessible::State changed;
            changed.disabled = true;
            QAccessibleStateChangeEvent event(this, changed);
            QAccessible::updateAccessibility(&event);
        }
        QAccessibleTextCursorEvent cursorEvent(this, accessibleCursorPosition());
        QAccessible::updateAccessibility(&cursorEvent);
        const auto selection = accessibleSelection();
        QAccessibleTextSelectionEvent selectionEvent(
            this,
            selection.first,
            selection.second);
        QAccessible::updateAccessibility(&selectionEvent);
    }
}

void TerminalView::sendFocus(const bool focused)
{
    m_desiredFocus = focused;
    dispatchFocus(
        focused ? FocusOperation::Focus : FocusOperation::Blur);
}

void TerminalView::dispatchFocus(const FocusOperation operation)
{
    const auto focused = operation == FocusOperation::Focus;
    if (operation == FocusOperation::None) {
        return;
    }
    if (focused && (!m_canSendInput || !interactionAvailable())) {
        return;
    }
    if (m_runtime == nullptr || !m_runtime->isRunning()) {
        return;
    }
    if (focused && (m_focusClaimed || !m_pendingFocusRequestId.isEmpty())) {
        return;
    }
    auto command = QByteArrayLiteral("{\"type\":");
    command += jsonStringBytes(
        focused ? QStringLiteral("session.focus") : QStringLiteral("session.blur"));
    command += QByteArrayLiteral(",\"sessionId\":");
    command += jsonStringBytes(m_subscription.sessionId);
    command += QByteArrayLiteral(",\"clientId\":");
    command += jsonStringBytes(m_subscription.subscriptionId);
    command += QByteArrayLiteral(",\"subscriptionGeneration\":");
    command += QByteArray::number(m_subscription.generation);
    if (focused) {
        m_pendingFocusRequestId =
            QUuid::createUuidV7().toString(QUuid::WithoutBraces);
        command += QByteArrayLiteral(",\"requestId\":");
        command += jsonStringBytes(m_pendingFocusRequestId);
    }
    command += QByteArrayLiteral(",\"expectedRuntimeIncarnationId\":");
    command += jsonStringBytes(m_expectedRuntimeIncarnationId);
    command += '}';
    if (auto result = m_runtime->send(command); !result) {
        if (focused) {
            m_pendingFocusRequestId.clear();
        }
        if (result.error().ffiResult == KODOSI_FFI_BUSY) {
            scheduleFocusRetry(operation);
        } else {
            m_focusRetryQueued = false;
            m_focusRetryOperation = FocusOperation::None;
            emit operationError(result.error().message);
        }
        return;
    }
    m_focusRetryQueued = false;
    m_focusRetryOperation = FocusOperation::None;
    if (!focused) {
        m_focusClaimed = false;
        m_pendingFocusRequestId.clear();
        if (m_desiredFocus && m_canSendInput) {
            sendFocus(true);
        }
    }
}

void TerminalView::scheduleFocusRetry(
    const FocusOperation operation)
{
    if (m_focusRetryQueued
        && m_focusRetryOperation == operation) {
        return;
    }
    const auto epoch = m_attachmentEpoch.load(std::memory_order_acquire);
    m_focusRetryQueued = true;
    m_focusRetryOperation = operation;
    QTimer::singleShot(16, this, [this, epoch] {
        if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) {
            return;
        }
        const auto operation = m_focusRetryOperation;
        m_focusRetryQueued = false;
        m_focusRetryOperation = FocusOperation::None;
        dispatchFocus(operation);
    });
}

void TerminalView::scheduleResizeRetry()
{
    if (m_resizeRetryQueued) {
        return;
    }
    const auto epoch = m_attachmentEpoch.load(std::memory_order_acquire);
    m_resizeRetryQueued = true;
    QTimer::singleShot(16, this, [this, epoch] {
        if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) {
            return;
        }
        m_resizeRetryQueued = false;
        dispatchResize();
    });
}

void TerminalView::handleFocusOutcome(TerminalFocusOutcome outcome)
{
    if (outcome.requestId != m_pendingFocusRequestId
        || outcome.runtimeIncarnationId != m_expectedRuntimeIncarnationId) {
        return;
    }
    m_pendingFocusRequestId.clear();
    m_focusClaimed = outcome.applied;
    if (!outcome.applied) {
        emit operationError(
            outcome.reason.isEmpty()
                ? QStringLiteral("The runtime rejected terminal focus.")
                : std::move(outcome.reason));
    }
}

void TerminalView::handleResizeOutcome(TerminalResizeOutcome outcome)
{
    if (!m_pendingResize
        || outcome.sessionId != m_subscription.sessionId
        || outcome.subscriptionId != m_subscription.subscriptionId
        || outcome.requestId != m_pendingResize->requestId
        || outcome.expectedRuntimeIncarnationId != m_expectedRuntimeIncarnationId
        || outcome.subscriptionGeneration != m_subscription.generation
        || outcome.surfaceGeneration != m_surfaceGeneration
        || outcome.columns != m_pendingResize->columns
        || outcome.rows != m_pendingResize->rows
        || outcome.widthPixels != m_pendingResize->widthPixels
        || outcome.heightPixels != m_pendingResize->heightPixels
        || outcome.cellWidthPixels != m_pendingResize->cellWidthPixels
        || outcome.cellHeightPixels != m_pendingResize->cellHeightPixels) {
        return;
    }
    const auto completedClaim = m_pendingResize->claim;
    if (outcome.applied) {
        m_lastResizeKey = m_pendingResize->key;
        if (completedClaim) {
            m_resizeClaimPending = false;
        }
    } else {
        emit operationError(
            outcome.reason.isEmpty()
                ? QStringLiteral("The runtime rejected terminal resize.")
                : std::move(outcome.reason));
    }
    m_pendingResize.reset();
    if (!completedClaim && m_resizeClaimPending) {
        scheduleResize();
    }
}

void TerminalView::scheduleResize()
{
    if (m_resizeQueued || m_runtime == nullptr || m_frame == nullptr) {
        return;
    }
    const auto epoch = m_attachmentEpoch.load(std::memory_order_acquire);
    m_resizeQueued = true;
    QTimer::singleShot(0, this, [this, epoch] {
        if (m_attachmentEpoch.load(std::memory_order_acquire) != epoch) {
            return;
        }
        m_resizeQueued = false;
        dispatchResize();
    });
}

void TerminalView::dispatchResize()
{
    if (m_runtime == nullptr || !m_runtime->isRunning() || m_surfaceGeneration == 0
        || !m_canResize || !interactionAvailable()
        || width() <= 0.0 || height() <= 0.0) {
        return;
    }
    const auto cell = TerminalRasterizer::cellSize(m_font, m_lineHeight);
    if (cell.width() <= 0.0 || cell.height() <= 0.0) {
        return;
    }
    const auto columns = static_cast<std::uint16_t>(std::clamp(
        std::floor(width() / cell.width()),
        1.0,
        static_cast<double>(std::numeric_limits<std::uint16_t>::max())));
    const auto rows = static_cast<std::uint16_t>(std::clamp(
        std::floor(height() / cell.height()),
        1.0,
        static_cast<double>(std::numeric_limits<std::uint16_t>::max())));
    const auto dpr = window() == nullptr ? 1.0 : window()->effectiveDevicePixelRatio();
    const auto cellWidthPixels = static_cast<std::uint32_t>(
        std::max(1LL, std::llround(cell.width() * dpr)));
    const auto cellHeightPixels = static_cast<std::uint32_t>(
        std::max(1LL, std::llround(cell.height() * dpr)));
    const auto widthProduct =
        static_cast<std::uint64_t>(columns) * cellWidthPixels;
    const auto heightProduct =
        static_cast<std::uint64_t>(rows) * cellHeightPixels;
    if (widthProduct > std::numeric_limits<std::uint32_t>::max()
        || heightProduct > std::numeric_limits<std::uint32_t>::max()) {
        emit operationError(QStringLiteral("Terminal pixel geometry exceeds the runtime limit."));
        return;
    }
    const auto widthPixels = static_cast<std::uint32_t>(widthProduct);
    const auto heightPixels = static_cast<std::uint32_t>(heightProduct);
    auto resizeKey = QByteArray::number(columns) + 'x' + QByteArray::number(rows)
        + ':' + QByteArray::number(widthPixels) + 'x' + QByteArray::number(heightPixels)
        + ':' + QByteArray::number(cellWidthPixels) + 'x' + QByteArray::number(cellHeightPixels);
    if ((!m_resizeClaimPending && resizeKey == m_lastResizeKey)
        || (m_pendingResize && resizeKey == m_pendingResize->key)) {
        return;
    }
    PendingResize pending {
        .key = std::move(resizeKey),
        .requestId = QUuid::createUuidV7().toString(QUuid::WithoutBraces),
        .columns = columns,
        .rows = rows,
        .widthPixels = widthPixels,
        .heightPixels = heightPixels,
        .cellWidthPixels = cellWidthPixels,
        .cellHeightPixels = cellHeightPixels,
        .claim = m_resizeClaimPending,
    };

    auto command = QByteArrayLiteral("{\"type\":\"session.resize\",\"sessionId\":");
    command += jsonStringBytes(m_subscription.sessionId);
    command += QByteArrayLiteral(",\"requestId\":");
    command += jsonStringBytes(pending.requestId);
    command += QByteArrayLiteral(",\"expectedRuntimeIncarnationId\":");
    command += jsonStringBytes(m_expectedRuntimeIncarnationId);
    command += QByteArrayLiteral(",\"subscriptionId\":");
    command += jsonStringBytes(m_subscription.subscriptionId);
    command += QByteArrayLiteral(",\"subscriptionGeneration\":");
    command += QByteArray::number(m_subscription.generation);
    command += QByteArrayLiteral(",\"surfaceGeneration\":");
    command += QByteArray::number(m_surfaceGeneration);
    command += QByteArrayLiteral(",\"cols\":");
    command += QByteArray::number(columns);
    command += QByteArrayLiteral(",\"rows\":");
    command += QByteArray::number(rows);
    command += QByteArrayLiteral(",\"widthPixels\":");
    command += QByteArray::number(widthPixels);
    command += QByteArrayLiteral(",\"heightPixels\":");
    command += QByteArray::number(heightPixels);
    command += QByteArrayLiteral(",\"cellWidthPixels\":");
    command += QByteArray::number(cellWidthPixels);
    command += QByteArrayLiteral(",\"cellHeightPixels\":");
    command += QByteArray::number(cellHeightPixels);
    command += pending.claim
        ? QByteArrayLiteral(",\"claim\":true}")
        : QByteArrayLiteral(",\"claim\":false}");
    m_pendingResize = std::move(pending);
    if (auto result = m_runtime->send(command); !result) {
        m_pendingResize.reset();
        if (result.error().ffiResult == KODOSI_FFI_BUSY) {
            scheduleResizeRetry();
        } else {
            emit operationError(result.error().message);
        }
    }
}

void TerminalView::presentFailure(GhosttyTerminalKernel::Failure failure)
{
    Q_ASSERT(thread() == QThread::currentThread());
    emit terminalError(std::move(failure.message));
}

TerminalKernelSettings TerminalView::kernelSettings() const
{
    return {
        .cursorStyle = m_cursorStyle,
        .cursorBlink = m_cursorBlink,
        .scrollbackLines = static_cast<std::size_t>(m_scrollbackLines),
        .scrollbackBytes = terminalScrollbackByteBudget(
            static_cast<std::size_t>(m_scrollbackLines)),
    };
}

void TerminalView::configureAttachedKernel()
{
    if (m_registry == nullptr) {
        return;
    }
    if (auto configured =
            m_registry->configure(surfaceIdentity(), kernelSettings());
        !configured) {
        emit operationError(configured.error().message);
    }
}

bool TerminalView::blinkEligible() const
{
    return m_cursorBlink && m_terminalReady && m_frame
        && m_frame->cursor.visible && hasActiveFocus() && interactionAvailable();
}

void TerminalView::updateBlinkTimer()
{
    if (blinkEligible()) {
        if (!m_cursorBlinkTimer.isActive()) {
            m_cursorPhaseVisible = true;
            m_cursorBlinkTimer.start();
        }
        return;
    }
    m_cursorBlinkTimer.stop();
    if (!m_cursorPhaseVisible) {
        m_cursorPhaseVisible = true;
        m_renderedFrame.reset();
        update();
    }
}

void TerminalView::invalidateMetrics()
{
    m_renderedFrame.reset();
    if (m_frame) {
        const auto sizeHint =
            TerminalRasterizer::logicalSize(*m_frame, m_font, m_lineHeight);
        setImplicitSize(sizeHint.width(), sizeHint.height());
    }
    updateViewport();
    scheduleResize();
    if (auto* inputMethod = QGuiApplication::inputMethod()) {
        inputMethod->update(Qt::ImFont | Qt::ImCursorRectangle);
    }
}

}
