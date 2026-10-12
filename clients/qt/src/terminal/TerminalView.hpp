#pragma once

#include "runtime/RuntimeBridge.hpp"
#include "terminal/TerminalRasterizer.hpp"
#include "terminal/TerminalSessionRegistry.hpp"

#include <QFont>
#include <QPoint>
#include <QQuickItem>
#include <QTimer>
#include <QUrl>
#include <QVariant>

#include <atomic>
#include <cstdint>
#include <deque>
#include <mutex>
#include <optional>
#include <variant>

namespace kodosi {

namespace detail {

class TerminalFrameMailbox final {
public:
    struct EnqueueResult {
        bool accepted = false;
        bool queueDrain = false;
    };

    void reset(std::uint64_t incarnation);
    [[nodiscard]] EnqueueResult enqueue(
        std::uint64_t incarnation,
        GhosttyTerminalKernel::Frame frame);
    [[nodiscard]] GhosttyTerminalKernel::Frame take(std::uint64_t incarnation);

private:
    std::mutex m_mutex;
    GhosttyTerminalKernel::Frame m_pendingFrame;
    std::uint64_t m_incarnation = 0;
    std::uint64_t m_highestDisplayRevision = 0;
    bool m_drainQueued = false;
};

}

class TerminalView : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QString fontFamily READ fontFamily WRITE setFontFamily NOTIFY fontFamilyChanged)
    Q_PROPERTY(int fontPixelSize READ fontPixelSize WRITE setFontPixelSize NOTIFY fontPixelSizeChanged)
    Q_PROPERTY(double lineHeight READ lineHeight WRITE setLineHeight NOTIFY lineHeightChanged)
    Q_PROPERTY(int cursorStyle READ cursorStyle WRITE setCursorStyle NOTIFY cursorStyleChanged)
    Q_PROPERTY(bool cursorBlink READ cursorBlink WRITE setCursorBlink NOTIFY cursorBlinkChanged)
    Q_PROPERTY(int scrollbackLines READ scrollbackLines WRITE setScrollbackLines NOTIFY scrollbackLinesChanged)
    Q_PROPERTY(QColor selectionBackground READ selectionBackground WRITE setSelectionBackground NOTIFY selectionBackgroundChanged)
    Q_PROPERTY(QColor selectionForeground READ selectionForeground WRITE setSelectionForeground NOTIFY selectionForegroundChanged)
    Q_PROPERTY(QColor preeditBackground READ preeditBackground WRITE setPreeditBackground NOTIFY preeditBackgroundChanged)
    Q_PROPERTY(QColor preeditForeground READ preeditForeground WRITE setPreeditForeground NOTIFY preeditForegroundChanged)
    Q_PROPERTY(qreal viewportScale READ viewportScale NOTIFY viewportChanged)
    Q_PROPERTY(QSizeF gridSize READ gridSize NOTIFY viewportChanged)
    Q_PROPERTY(bool terminalReady READ terminalReady NOTIFY terminalReadyChanged)
    Q_PROPERTY(bool canSendInput READ canSendInput NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool canResize READ canResize NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool readOnly READ readOnly NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasSelection READ hasSelection NOTIFY frameChanged)
    Q_PROPERTY(
        bool focusedSizeAuthority
        READ focusedSizeAuthority
        WRITE setFocusedSizeAuthority
        NOTIFY focusedSizeAuthorityChanged)

public:
    explicit TerminalView(QQuickItem* parent = nullptr);
    ~TerminalView() override;

    [[nodiscard]] QString fontFamily() const;
    [[nodiscard]] int fontPixelSize() const noexcept;
    [[nodiscard]] double lineHeight() const noexcept;
    [[nodiscard]] int cursorStyle() const noexcept;
    [[nodiscard]] bool cursorBlink() const noexcept;
    [[nodiscard]] int scrollbackLines() const noexcept;
    [[nodiscard]] QColor selectionBackground() const;
    [[nodiscard]] QColor selectionForeground() const;
    [[nodiscard]] QColor preeditBackground() const;
    [[nodiscard]] QColor preeditForeground() const;
    [[nodiscard]] qreal viewportScale() const;
    [[nodiscard]] QSizeF gridSize() const;
    [[nodiscard]] bool terminalReady() const noexcept;
    [[nodiscard]] bool canSendInput() const noexcept;
    [[nodiscard]] bool canResize() const noexcept;
    [[nodiscard]] bool readOnly() const noexcept;
    [[nodiscard]] bool hasSelection() const;
    [[nodiscard]] bool focusedSizeAuthority() const noexcept;
    [[nodiscard]] QString accessibleText() const;
    [[nodiscard]] int accessibleCursorPosition() const;
    [[nodiscard]] QPair<int, int> accessibleSelection() const;
    [[nodiscard]] QRect accessibleCharacterRect(int offset) const;
    [[nodiscard]] int accessibleOffsetAt(const QPoint& globalPoint) const;
    void setFontFamily(const QString& family);
    void setFontPixelSize(int size);
    void setLineHeight(double lineHeight);
    void setCursorStyle(int style);
    void setCursorBlink(bool enabled);
    void setScrollbackLines(int lines);
    void setSelectionBackground(const QColor& color);
    void setSelectionForeground(const QColor& color);
    void setPreeditBackground(const QColor& color);
    void setPreeditForeground(const QColor& color);
    void setTerminalInteraction(bool canSendInput, bool canResize);
    void setFocusedSizeAuthority(bool focused);

    [[nodiscard]] bool attach(
        TerminalSessionRegistry& registry,
        TerminalCommandDispatcher& runtime,
        TerminalSubscription subscription,
        QString expectedRuntimeIncarnationId,
        std::uint64_t surfaceGeneration = 1);
    Q_INVOKABLE void detach();
    Q_INVOKABLE [[nodiscard]] bool copySelectionToClipboard();
    Q_INVOKABLE void pasteFromClipboard();

signals:
    void fontFamilyChanged();
    void fontPixelSizeChanged();
    void lineHeightChanged();
    void cursorStyleChanged();
    void cursorBlinkChanged();
    void scrollbackLinesChanged();
    void selectionBackgroundChanged();
    void selectionForegroundChanged();
    void preeditBackgroundChanged();
    void preeditForegroundChanged();
    void viewportChanged();
    void terminalReadyChanged();
    void capabilitiesChanged();
    void focusedSizeAuthorityChanged();
    void frameChanged();
    void terminalError(QString message);
    void operationError(QString message);
    void contextMenuRequested(qreal x, qreal y);
    void connectionCompleted(bool connected, std::int32_t result);
    void terminalClosed();

protected:
    bool event(QEvent* event) override;
    QSGNode* updatePaintNode(
        QSGNode* oldNode,
        UpdatePaintNodeData* updateData) override;
    void geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;
    void keyPressEvent(QKeyEvent* event) override;
    void keyReleaseEvent(QKeyEvent* event) override;
    void inputMethodEvent(QInputMethodEvent* event) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery query) const override;
    void focusInEvent(QFocusEvent* event) override;
    void focusOutEvent(QFocusEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void hoverMoveEvent(QHoverEvent* event) override;
    void hoverLeaveEvent(QHoverEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;

private:
    enum class CellHitTest {
        Strict,
        ClampToViewport,
    };

    enum class FocusOperation {
        None,
        Focus,
        Blur,
    };

    struct PendingResize {
        QByteArray key;
        QString requestId;
        std::uint16_t columns;
        std::uint16_t rows;
        std::uint32_t widthPixels;
        std::uint32_t heightPixels;
        std::uint32_t cellWidthPixels;
        std::uint32_t cellHeightPixels;
        bool claim = false;
    };

    [[nodiscard]] QPointF viewportOffset() const;
    [[nodiscard]] QPointF gridPoint(const QPointF& point) const;
    void updateViewport();
    QFont m_font;
    qreal m_lineHeight = 1.1;
    TerminalCursorStyle m_cursorStyle = TerminalCursorStyle::Block;
    bool m_cursorBlink = false;
    int m_scrollbackLines = 10'000;
    TerminalRasterizer::Palette m_palette;
    QTimer m_cursorBlinkTimer;
    QMetaObject::Connection m_windowVisibilityConnection;
    bool m_cursorPhaseVisible = true;
    GhosttyTerminalKernel::Frame m_frame;
    GhosttyTerminalKernel::Frame m_renderedFrame;
    TerminalSessionRegistry* m_registry = nullptr;
    TerminalCommandDispatcher* m_runtime = nullptr;
    TerminalSubscription m_subscription;
    QString m_expectedRuntimeIncarnationId;
    qreal m_renderedDevicePixelRatio = 0.0;
    qreal m_renderedViewportScale = 0.0;
    int m_renderedFontPixelSize = 0;
    QString m_preedit;
    QString m_renderedPreedit;
    std::uint64_t m_surfaceGeneration = 0;
    bool m_resizeQueued = false;
    QByteArray m_lastResizeKey;
    std::optional<PendingResize> m_pendingResize;
    QString m_pendingFocusRequestId;
    bool m_focusClaimed = false;
    bool m_terminalReady = false;
    bool m_focusRetryQueued = false;
    FocusOperation m_focusRetryOperation = FocusOperation::None;
    bool m_resizeRetryQueued = false;
    bool m_desiredFocus = false;
    bool m_canSendInput = false;
    bool m_canResize = false;
    bool m_focusedSizeAuthority = false;
    bool m_resizeClaimPending = false;
    std::deque<QByteArray> m_inputQueue;
    qsizetype m_queuedInputBytes = 0;
    bool m_inputRetryQueued = false;
    std::uint8_t m_inputBackoffStep = 0;
    std::atomic<std::uint64_t> m_attachmentEpoch {0};
    detail::TerminalFrameMailbox m_frameMailbox;
    struct Closed {};
    using GuiEvent = std::variant<GhosttyTerminalKernel::Failure, std::int32_t,
        TerminalFocusOutcome, TerminalResizeOutcome, Closed>;
    std::mutex m_guiEventMutex;
    std::deque<GuiEvent> m_guiEvents;
    bool m_guiDrainQueued = false;
    bool m_guiOverflow = false;
    std::uint64_t m_inputGeneration = 0;
    bool m_selecting = false;
    Qt::MouseButton m_reportedMouseButton = Qt::NoButton;
    bool m_copyShortcutActive = false;
    std::optional<int> m_pasteShortcutKey;
    std::optional<QUrl> m_pressedLink;
    int m_wheelAngleRemainder = 0;
    qreal m_wheelPixelRemainder = 0.0;
    std::atomic_bool m_renderPerformanceQueued = false;
    std::atomic<std::uint64_t> m_renderPerformanceGeneration {1};

    void enqueueFrame(std::uint64_t epoch, GhosttyTerminalKernel::Frame frame);
    void enqueueGuiEvent(std::uint64_t epoch, GuiEvent event);
    void drainGuiEvents(std::uint64_t epoch);
    [[nodiscard]] bool interactionAvailable() const;
    void synchronizeInteraction();
    void clearPendingInput();
    void queueRenderPerformance(qint64 durationMilliseconds, QString outcome);
    void drainFrame(std::uint64_t epoch);
    void presentFrame(GhosttyTerminalKernel::Frame frame);
    void presentFailure(GhosttyTerminalKernel::Failure failure);
    [[nodiscard]] TerminalSurfaceIdentity surfaceIdentity() const;
    void sendText(const QString& text);
    void pasteClipboard();
    [[nodiscard]] bool sendKey(QKeyEvent* event, TerminalKeyAction action);
    [[nodiscard]] bool enqueueInput(QByteArray bytes);
    static constexpr int inputBackoffMilliseconds[] {4, 8, 16, 32, 64};
    void drainInputQueue();
    void sendFocus(bool focused);
    void dispatchFocus(FocusOperation operation);
    void scheduleFocusRetry(FocusOperation operation);
    void scheduleResizeRetry();
    void handleFocusOutcome(TerminalFocusOutcome outcome);
    void handleResizeOutcome(TerminalResizeOutcome outcome);
    void scheduleResize();
    void dispatchResize();
    [[nodiscard]] std::optional<QPoint> terminalCellAt(
        const QPointF& position,
        CellHitTest hitTest = CellHitTest::Strict) const;
    [[nodiscard]] std::optional<QUrl> linkAtCell(
        const QPoint& cell,
        bool reportFailure);
    void updateHoverLink(const QPointF& position);
    void clearHoverLink();
    [[nodiscard]] int wheelRows(QWheelEvent* event);
    void beginSelection(const QPoint& anchor);
    void updateSelection(const QPoint& endpoint);
    [[nodiscard]] bool copySelection();
    [[nodiscard]] bool sendMouseEvent(
        TerminalMouseAction action,
        TerminalMouseButton button,
        const QPointF& position,
        Qt::KeyboardModifiers modifiers,
        bool anyButtonPressed);
    [[nodiscard]] TerminalKernelSettings kernelSettings() const;
    void configureAttachedKernel();
    void updateBlinkTimer();
    [[nodiscard]] bool blinkEligible() const;
    void invalidateMetrics();
};

}
