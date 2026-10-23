#include "terminal/GhosttyTerminalKernel.hpp"
#include "terminal/GhosttyC.hpp"
#include "terminal/TerminalLink.hpp"
#include "terminal/TerminalAccessibility.hpp"
#include "terminal/TerminalRasterizer.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include "terminal/TerminalView.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "SessionFixture.hpp"

#include <QAccessible>
#include <QByteArray>
#include <QClipboard>
#include <QDesktopServices>
#include <QFocusEvent>
#include <QFontDatabase>
#include <QInputMethodEvent>
#include <QHoverEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QRawFont>
#include <QSignalSpy>
#include <QThread>
#include <QTimer>
#include <QUrl>
#include <QWheelEvent>
#include <QtTest/QTest>

#include <cmath>
#include <cstdint>
#include <functional>
#include <optional>
#include <utility>
#include <vector>

namespace {

constexpr std::int32_t ffiOk = 0;
constexpr std::int32_t ffiBusy = 6;
constexpr std::int32_t ffiSessionNotFound = 7;

class FakeTerminalDispatcher final
    : public kodosi::TerminalCommandDispatcher {
public:
    bool running = true;
    int connectCount = 0;
    int disconnectCount = 0;
    int busyTerminalCommands = 0;
    int unconnectedTerminalCommands = 0;
    int busyInputCommands = 0;
    QVector<QJsonObject> terminalCommands;
    QVector<QByteArray> inputCommands;
    QVector<kodosi::TerminalSubscription> inputSubscriptions;
    QVector<QString> inputIncarnations;

    bool isRunning() const noexcept override
    {
        return running;
    }

    Result send(
        const QByteArrayView json) override
    {
        const auto document =
            QJsonDocument::fromJson(json.toByteArray());
        if (document.isObject()) {
            terminalCommands.append(document.object());
        }
        if (busyTerminalCommands > 0) {
            --busyTerminalCommands;
            return busyFailure();
        }
        if (unconnectedTerminalCommands > 0) {
            --unconnectedTerminalCommands;
            return std::unexpected(kodosi::RuntimeFailure {
                .ffiResult = ffiSessionNotFound,
                .message = QStringLiteral("Command failed (7)."),
            });
        }
        return {};
    }

    Result connectTerminal(
        const kodosi::TerminalSubscription&) override
    {
        ++connectCount;
        return {};
    }

    Result refreshTerminal(
        const kodosi::TerminalSubscription&) override
    {
        return {};
    }

    Result disconnectTerminal(
        const kodosi::TerminalSubscription&) override
    {
        ++disconnectCount;
        return {};
    }

    Result sendTerminalInput(
        const kodosi::TerminalSubscription& subscription,
        const QString& incarnation,
        const QByteArrayView bytes) override
    {
        inputCommands.append(bytes.toByteArray());
        inputSubscriptions.append(subscription);
        inputIncarnations.append(incarnation);
        if (busyInputCommands > 0) {
            --busyInputCommands;
            return busyFailure();
        }
        return {};
    }

private:
    static Result busyFailure()
    {
        return std::unexpected(kodosi::RuntimeFailure {
            .ffiResult = ffiBusy,
            .message = QStringLiteral("Busy"),
        });
    }
};

class UrlCapture final : public QObject {
    Q_OBJECT

public:
    QList<QUrl> values;

public slots:
    void capture(const QUrl& url)
    {
        values.append(url);
    }
};

class InspectableTerminalView final : public kodosi::TerminalView {
public:
    using TerminalView::TerminalView;
    using TerminalView::inputMethodQuery;
};

class FakeRuntimeBridge final : public kodosi::RuntimeBridge {
public:
    explicit FakeRuntimeBridge(kodosi::TerminalEventSink& sink)
        : RuntimeBridge(sink)
    {
    }

    bool isRunning() const noexcept override
    {
        return true;
    }

    Result send(
        QByteArrayView) override
    {
        return {};
    }

    Result connectTerminal(
        const kodosi::TerminalSubscription& subscription) override
    {
        ++connectCount;
        connectedSubscriptions.append(subscription);
        return {};
    }

    Result refreshTerminal(
        const kodosi::TerminalSubscription& subscription) override
    {
        ++refreshCount;
        refreshedSubscriptions.append(subscription);
        if (refreshFailures > 0) {
            --refreshFailures;
            return std::unexpected(kodosi::RuntimeFailure {
                .ffiResult = ffiBusy,
                .message = QStringLiteral("Busy"),
            });
        }
        if (refreshHandler) {
            refreshHandler(subscription);
        }
        return {};
    }

    Result disconnectTerminal(
        const kodosi::TerminalSubscription& subscription) override
    {
        ++disconnectCount;
        disconnectedSubscriptions.append(subscription);
        return {};
    }

    Result sendTerminalInput(
        const kodosi::TerminalSubscription&,
        const QString&,
        QByteArrayView) override
    {
        ++inputCount;
        return {};
    }

    int connectCount = 0;
    int refreshCount = 0;
    int refreshFailures = 0;
    int disconnectCount = 0;
    int inputCount = 0;
    QVector<kodosi::TerminalSubscription> connectedSubscriptions;
    QVector<kodosi::TerminalSubscription> refreshedSubscriptions;
    QVector<kodosi::TerminalSubscription> disconnectedSubscriptions;
    std::function<void(const kodosi::TerminalSubscription&)> refreshHandler;
};

QByteArray checkpointFor(const QByteArray& bytes, const std::uint16_t columns, const std::uint16_t rows)
{
    GhosttyTerminal terminal = nullptr;
    if (ghostty_terminal_new(nullptr, &terminal, columns, rows) != GHOSTTY_SUCCESS) {
        QTest::qFail("failed to create checkpoint terminal", __FILE__, __LINE__);
        return {};
    }
    ghostty_terminal_vt_write(
        terminal,
        reinterpret_cast<const std::uint8_t*>(bytes.constData()),
        static_cast<std::size_t>(bytes.size()));

    std::size_t required = 0;
    if (ghostty_snapshot_encode_buf(terminal, nullptr, 0, &required) != GHOSTTY_OUT_OF_SPACE) {
        ghostty_terminal_free(terminal);
        QTest::qFail("failed to size snapshot", __FILE__, __LINE__);
        return {};
    }
    QByteArray snapshot(static_cast<qsizetype>(required), Qt::Uninitialized);
    std::size_t written = 0;
    if (ghostty_snapshot_encode_buf(
            terminal, reinterpret_cast<std::uint8_t*>(snapshot.data()), required, &written)
        != GHOSTTY_SUCCESS) {
        ghostty_terminal_free(terminal);
        QTest::qFail("failed to encode snapshot", __FILE__, __LINE__);
        return {};
    }
    snapshot.resize(static_cast<qsizetype>(written));
    ghostty_terminal_free(terminal);
    return snapshot;
}

QByteArray imageCheckpoint()
{
    GhosttyTerminal terminal = nullptr;
    if (ghostty_terminal_new(nullptr, &terminal, 10, 4) != GHOSTTY_SUCCESS) {
        QTest::qFail("failed to create image terminal", __FILE__, __LINE__);
        return {};
    }
    const std::uint64_t storageLimit = 64ULL << 20;
    ghostty_terminal_set(terminal, GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT, &storageLimit);
    ghostty_terminal_resize(terminal, 10, 4, 2, 4);
    QByteArray red;
    for (auto pixel = 0; pixel < 4 * 8; ++pixel) {
        red += QByteArray::fromHex("ff0000");
    }
    const auto output = "\x1b[2;4H\x1b_Ga=T,f=24,s=4,v=8,i=7,q=2;" + red.toBase64() + "\x1b\\";
    ghostty_terminal_vt_write(
        terminal,
        reinterpret_cast<const std::uint8_t*>(output.constData()),
        static_cast<std::size_t>(output.size()));
    QByteArray snapshot;
    const GhosttyWriter writer {
        [](void* const target, const std::uint8_t* const data, const std::size_t length) {
            static_cast<QByteArray*>(target)->append(
                reinterpret_cast<const char*>(data),
                static_cast<qsizetype>(length));
            return true;
        },
        &snapshot,
    };
    const auto result = ghostty_snapshot_encode(terminal, writer);
    ghostty_terminal_free(terminal);
    if (result != GHOSTTY_SUCCESS) {
        QTest::qFail("failed to encode image snapshot", __FILE__, __LINE__);
        return {};
    }
    return snapshot;
}

constexpr std::uint16_t readyRecord = 5;
constexpr std::uint16_t pixelsRecord = 9;

struct SnapshotRecord {
    std::uint16_t tag = 0;
    QByteArray bytes;
};

std::vector<SnapshotRecord> snapshotRecords(const QByteArray& snapshot)
{
    std::vector<SnapshotRecord> records;
    for (qsizetype at = 10; at + 10 <= snapshot.size();) {
        const auto byte = [&](const qsizetype offset) {
            return static_cast<std::uint32_t>(static_cast<std::uint8_t>(snapshot[at + offset]));
        };
        const auto length = static_cast<qsizetype>(
            byte(2) | (byte(3) << 8) | (byte(4) << 16) | (byte(5) << 24));
        records.push_back({
            .tag = static_cast<std::uint16_t>(byte(0) | (byte(1) << 8)),
            .bytes = snapshot.mid(at, 10 + length),
        });
        at += 10 + length;
    }
    return records;
}

QByteArray withoutPixels(const QByteArray& snapshot)
{
    auto kept = snapshot.left(10);
    for (const auto& record : snapshotRecords(snapshot)) {
        if (record.tag != pixelsRecord) {
            kept += record.bytes;
        }
    }
    return kept;
}

QByteArray visiblePart(const QByteArray& snapshot)
{
    auto visible = snapshot.left(10);
    auto ready = false;
    for (const auto& record : snapshotRecords(snapshot)) {
        if (ready && record.tag != pixelsRecord) {
            break;
        }
        visible += record.bytes;
        ready = ready || record.tag == readyRecord;
    }
    return visible;
}

QString frameText(const kodosi::TerminalFrame& frame)
{
    QString text;
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        for (std::uint16_t column = 0; column < frame.columns; ++column) {
            const auto* cell = frame.cell(column, row);
            if (cell != nullptr && cell->width != 0) {
                text += cell->grapheme.isEmpty() ? QStringLiteral(" ") : cell->grapheme;
            }
        }
        text += QLatin1Char('\n');
    }
    return text;
}

QByteArray markedPrompts(const int count)
{
    QByteArray output;
    for (int prompt = 0; prompt < count; ++prompt) {
        const auto number = QByteArray::number(prompt);
        output += "\x1b]133;A\x07$ \x1b]133;B\x07" "cmd-" + number + "\r\n\x1b]133;C\x07";
        output += "out-" + number + "a\r\nout-" + number + "b\r\n\x1b]133;D;0\x07";
    }
    return output + "\x1b]133;A\x07$ \x1b]133;B\x07";
}

void makeTerminalReady(
    kodosi::TerminalSessionRegistry& registry,
    const kodosi::TerminalSubscription& subscription)
{
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        24,
        80,
        checkpointFor(QByteArrayLiteral("ready"), 80, 24),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();
}

void completeResize(
    kodosi::TerminalSessionRegistry& registry,
    const kodosi::TerminalSubscription& subscription,
    QJsonObject command)
{
    command.insert(
        QStringLiteral("type"),
        QStringLiteral("term.resizeApplied"));
    registry.receiveControl({
        subscription,
        QJsonDocument(command).toJson(QJsonDocument::Compact),
    });
    QCoreApplication::processEvents();
}

}

class TerminalKernelTest final : public QObject {
    Q_OBJECT
    QQuickWindow m_window;

private slots:
    void initTestCase();
    void checkpointPrecedesContiguousRawData();
    void failedCheckpointPreservesPriorFrame();
    void resizeRequiresExactSequenceBoundary();
    void consoleCheckpointKeepsHistoryOnResize();
    void synchronizedOutputShowsOnlyFinishedFrames();
    void synchronizedOutputThatNeverEndsShowsAfterOneSecond();
    void kittyImagesCoverTheHostsCells();
    void kittyAnimationsPlayTheirFrames();
    void checkpointsCarryImagesWithinTheirPixelBudget();
    void placeholderCellsShowTheirPartOfTheImage();
    void promptJumpsMoveBetweenMarkedPrompts();
    void promptJumpKeysMoveTheViewWithoutTerminalInput();
    void historyCompressesAndKeepsItsText();
    void registryConsumesNormativeControlWireWithoutLosingU64Precision();
    void rasterizerPreservesTerminalCellBackgrounds();
    void rasterizerUsesBrandedSelectionAndPreeditPalette();
    void rasterizerHandlesWideTailBackgroundAndCursorSpan();
    void rasterizerAppliesLineHeightAndOwnedCursorPolicy();
    void rasterizerAlignsCellsToDevicePixels();
    void rasterizerFillsBlockAndPowerlineCells();
    void rasterizerDrawsImagesInTheirLayers();
    void rasterizerPaintsChangedRowsAsTheWholeFrame();
    void rasterizerDrawsARowAlikeInEveryRow();
    void kernelAppliesNativeCursorDefaultsAndScrollback();
    void scrollbackReconfigurationPublishesReplacementFrame();
    void terminalViewOwnsValidatedDisplaySettings();
    void terminalViewPrefersInstalledNerdFontFallback();
    void terminalViewOwnsExplicitSelectionAndPreeditPalette();
    void keyEncodingUsesRestoredTerminalModes();
    void mouseEncodingUsesRestoredTerminalModes();
    void terminalViewRoutesTrackedMouseToPty();
    void remoteGridFitsAndMapsAccessibleMouseCoordinates();
    void terminalViewPreservesControlAndEscapeKeys();
    void modifierKeysWithControlTextSendNothing();
    void runtimeBellAndTitleKeepTerminalAvailable();
    void hiddenAndDisabledTerminalRejectsInputAndCancelsRetries();
    void terminalControlsStayBoundedAndFenceReattachment();
    void terminalClosePresentsPendingFinalFrameBeforeDetaching();
    void retiredTerminalCloseCannotDetachReplacement();
    void pasteEncodingUsesRestoredTerminalModes();
    void wheelScrollsGhosttyViewportWithoutChangingSequence();
    void selectionUsesGhosttyTrackedStateAndFormatting();
    void aSelectionShowsOnlyAfterThePointerLeavesItsFirstCell();
    void framesDrawTheKodosiTerminalColors();
    void aWebAddressInTheTextIsALinkThatEndsBeforePunctuation();
    void scrolledSelectionUsesVisibleViewportCoordinates();
    void selectionEndpointsTrackIncomingOutput();
    void terminalViewExposesNativeAccessibleTextInterface();
    void qmlConstructionEnrollsTerminalInAccessibleTree();
    void terminalViewGatesDeniedCommandsAndRevocation();
    void terminalViewJoinsSurrogateHalvesFromSeparateKeyEvents();
    void terminalViewPastesClipboardThroughBoundedAuthority();
    void terminalViewTracksShiftInsertPasteReleaseExactly();
    void terminalProgramsWriteTheClipboardAndTheFocusedViewOfTheirOwnTerminalAnswersReads();
    void terminalProgramsReadTheClipboardOnlyWhenAllowed();
    void terminalViewWheelUpdatesFrameWithoutPtyInput();
    void terminalViewAcceptedInputReturnsViewportToBottom();
    void terminalViewSelectionAnchorSurvivesPresentedScroll();
    void terminalViewCancelsSelectionAgainstStaleDisplayedViewport();
    void terminalViewMailboxRejectsReversedDisplayRevisions();
    void terminalLinkValidationRejectsMalformedValues();
    void terminalViewActivatesGhosttyLinks();
    void terminalViewOpensAWebAddressInTheTextWithControlClick();
    void terminalViewCannotActivateLinkFromUndisplayedFrame();
    void terminalViewCancelsLinkReleaseOutsideBounds();
    void terminalViewCopySelectionStillUsesHostClipboard();
    void terminalViewRetriesRevocationBlurBeforeRefocus();
    void terminalViewSendsItsSizeOnlyWhileItMayResize();
    void terminalViewWaitsQuietlyForItsConnection();
    void terminalViewDefersResizeWhileEffectivelyHidden();
    void surfaceControllerFencesSessionIncarnations();
    void surfaceControllerKeepsBindingsIndependent();
    void surfaceControllerKeepsSameSessionSurfacesIndependent();
    void surfaceControllerRetriesFailedSeedRefresh();
    void surfaceControllerTimesOutAcceptedSeedRefresh();
    void surfaceControllerScopesAttachmentOutcomes();
    void registryRetiresAbandonedSeedCapacity();
    void registryRoutesMultiSurfaceControlExactly();
    void registryRestoresOpenSurfacesFromMovedSnapshot();
};

void TerminalKernelTest::terminalViewPreservesControlAndEscapeKeys()
{
    QQuickWindow window;
    window.resize(800, 500);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(800, 500));
    window.show();
    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(view.hasActiveFocus());
    for (const auto key : {Qt::Key_B, Qt::Key_S, Qt::Key_I}) {
        QKeyEvent event(QEvent::ShortcutOverride, key, Qt::ControlModifier);
        event.ignore();
        QCoreApplication::sendEvent(&view, &event);
        QVERIFY(event.isAccepted());
    }
    QKeyEvent escape(QEvent::ShortcutOverride, Qt::Key_Escape, Qt::NoModifier);
    escape.ignore();
    QCoreApplication::sendEvent(&view, &escape);
    QVERIFY(escape.isAccepted());
    QKeyEvent applicationShortcut(QEvent::ShortcutOverride, Qt::Key_N,
        Qt::ControlModifier | Qt::ShiftModifier);
    applicationShortcut.ignore();
    QCoreApplication::sendEvent(&view, &applicationShortcut);
    QVERIFY(!applicationShortcut.isAccepted());
}

void TerminalKernelTest::modifierKeysWithControlTextSendNothing()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(400, 200);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(400, 200));
    view.setTerminalInteraction(true, true);
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("modifier-keys"),
        QStringLiteral("modifier-keys-subscription"),
        1,
    };
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    window.show();
    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(view.hasActiveFocus());

    for (const auto key : {Qt::Key_Control, Qt::Key_Shift, Qt::Key_Alt}) {
        QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier, QString(QChar(0)));
        QCoreApplication::sendEvent(&view, &press);
        QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier, QString(QChar(0)));
        QCoreApplication::sendEvent(&view, &release);
    }
    QKeyEvent letter(QEvent::KeyPress, Qt::Key_unknown, Qt::NoModifier, QStringLiteral("\u00e9"));
    QCoreApplication::sendEvent(&view, &letter);
    QTRY_COMPARE(dispatcher.inputCommands.size(), 1);
    QCOMPARE(dispatcher.inputCommands.front(), QStringLiteral("\u00e9").toUtf8());
    window.hide();
}

void TerminalKernelTest::runtimeBellAndTitleKeepTerminalAvailable()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"), QStringLiteral("subscription"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    QSignalSpy errors(&view, &kodosi::TerminalView::terminalError);
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.bell","sessionId":"session"})")});
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.title","sessionId":"session","title":"Building project"})")});
    QCOMPARE(errors.count(), 0);
    QVERIFY(view.terminalReady());
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.title","sessionId":"session","title":null})")});
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.bell","sessionId":"wrong"})")});
    QTRY_COMPARE(errors.count(), 1);
}

void TerminalKernelTest::hiddenAndDisabledTerminalRejectsInputAndCancelsRetries()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(800, 500);
    QQuickItem container(window.contentItem());
    container.setSize(QSizeF(800, 500));
    InspectableTerminalView view(m_window.contentItem());
    view.setParentItem(&container);
    view.setSize(container.size());
    view.setTerminalInteraction(true, true);
    const kodosi::TerminalSubscription subscription {QStringLiteral("hidden-input"), QStringLiteral("subscription"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    window.show();
    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(view.hasActiveFocus());
    QTest::keyClick(&window, Qt::Key_A);
    QCOMPARE(dispatcher.inputCommands.size(), 1);
    dispatcher.busyInputCommands = 100;
    QTest::keyClick(&window, Qt::Key_B);
    const auto beforeHidden = dispatcher.inputCommands.size();
    container.setVisible(false);
    QTest::keyClick(&window, Qt::Key_C);
    QInputMethodEvent hiddenIme;
    hiddenIme.setCommitString(QStringLiteral("hidden"));
    QCoreApplication::sendEvent(&view, &hiddenIme);
    QVERIFY(!view.inputMethodQuery(Qt::ImEnabled).toBool());
    QTest::qWait(80);
    QCOMPARE(dispatcher.inputCommands.size(), beforeHidden);
    container.setVisible(true);
    dispatcher.busyInputCommands = 0;
    view.forceActiveFocus();
    QTest::keyClick(&window, Qt::Key_D);
    QCOMPARE(dispatcher.inputCommands.last(), QByteArrayLiteral("d"));
    QCOMPARE(dispatcher.inputCommands.size(), beforeHidden + 1);
    container.setEnabled(false);
    QCoreApplication::sendEvent(&view, &hiddenIme);
    QTest::qWait(80);
    QCOMPARE(dispatcher.inputCommands.size(), beforeHidden + 1);
    container.setEnabled(true);
    window.hide();
    QCoreApplication::sendEvent(&view, &hiddenIme);
    QCOMPARE(dispatcher.inputCommands.size(), beforeHidden + 1);
    QVERIFY(view.terminalReady());
    QCOMPARE(dispatcher.disconnectCount, 0);
}

void TerminalKernelTest::terminalControlsStayBoundedAndFenceReattachment()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {QStringLiteral("effects"), QStringLiteral("subscription"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    QSignalSpy errors(&view, &kodosi::TerminalView::terminalError);
    for (int i = 0; i < 5000; ++i) {
        registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"invalid"})")});
    }
    QCoreApplication::processEvents();
    QCOMPARE(errors.count(), 1);
    QVERIFY(!view.terminalReady());
    QCOMPARE(dispatcher.disconnectCount, 1);
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("replacement")));
    makeTerminalReady(registry, subscription);
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"invalid"})")});
    view.detach();
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("newer")));
    makeTerminalReady(registry, subscription);
    QCoreApplication::processEvents();
    QCOMPARE(errors.count(), 1);

}

void TerminalKernelTest::terminalClosePresentsPendingFinalFrameBeforeDetaching()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    view.setSize(QSizeF(640, 480));
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("final-output"), QStringLiteral("subscription"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    QStringList presentation;
    connect(&view, &kodosi::TerminalView::frameChanged, &view, [&] {
        if (view.accessibleText().contains(QStringLiteral("FINAL-MARKER")))
            presentation.append(QStringLiteral("frame"));
    });
    connect(&view, &kodosi::TerminalView::terminalReadyChanged, &view, [&] {
        if (!view.terminalReady()) presentation.append(QStringLiteral("detached"));
    });
    connect(&view, &kodosi::TerminalView::terminalClosed, &view, [&] {
        presentation.append(QStringLiteral("closed"));
    });
    registry.receiveControl({subscription, QByteArrayLiteral(
        R"({"type":"term.focusApplied","requestId":"focus","runtimeIncarnationId":"incarnation"})")});
    registry.receiveData({subscription, 1, QByteArrayLiteral(" FINAL-MARKER")});
    registry.receiveControl({subscription, QByteArrayLiteral(
        R"({"type":"term.closed","reason":"end","finalSequence":2})")});
    QVERIFY(presentation.isEmpty());
    QCoreApplication::processEvents();
    QCOMPARE(presentation, QStringList({QStringLiteral("frame"), QStringLiteral("detached"), QStringLiteral("closed")}));
    QCOMPARE(dispatcher.disconnectCount, 1);
}

void TerminalKernelTest::retiredTerminalCloseCannotDetachReplacement()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    view.setSize(QSizeF(640, 480));
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("replaced-output"), QStringLiteral("old-subscription"), 1};
    const kodosi::TerminalSubscription replacement {
        subscription.sessionId, QStringLiteral("new-subscription"), 2};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("old-incarnation")));
    makeTerminalReady(registry, subscription);
    QSignalSpy closed(&view, &kodosi::TerminalView::terminalClosed);
    bool replaced = false;
    connect(&view, &kodosi::TerminalView::frameChanged, &view, [&] {
        if (replaced || !view.accessibleText().contains(QStringLiteral("FINAL-MARKER"))) return;
        replaced = true;
        QVERIFY(view.attach(registry, dispatcher, replacement, QStringLiteral("new-incarnation")));
        QVERIFY(registry.installCheckpoint({replacement, 1, 24, 80,
            checkpointFor(QByteArrayLiteral("replacement"), 80, 24)}));
        registry.receiveConnectResult({replacement, ffiOk});
    });
    registry.receiveControl({subscription, QByteArrayLiteral(
        R"({"type":"term.focusApplied","requestId":"focus","runtimeIncarnationId":"old-incarnation"})")});
    registry.receiveData({subscription, 1, QByteArrayLiteral(" FINAL-MARKER")});
    registry.receiveControl({subscription, QByteArrayLiteral(
        R"({"type":"term.closed","reason":"end","finalSequence":2})")});
    QTRY_VERIFY(replaced && view.terminalReady());
    QCOMPARE(closed.count(), 0);
    QVERIFY(view.accessibleText().contains(QStringLiteral("replacement")));
    QCOMPARE(dispatcher.disconnectCount, 1);
}

void TerminalKernelTest::initTestCase()
{
    qRegisterMetaType<kodosi::TerminalView*>();
    m_window.resize(900, 600);
    m_window.show();
}

void TerminalKernelTest::terminalViewPrefersInstalledNerdFontFallback()
{
    const auto supportsTerminalSymbols = [](const QString& family) {
        auto font = QFont(family);
        font.setPixelSize(14);
        font.setStyleStrategy(QFont::NoFontMerging);
        const auto rawFont = QRawFont::fromFont(font);
        return rawFont.isValid()
            && rawFont.supportsCharacter(char32_t {0xE0B0})
            && rawFont.supportsCharacter(char32_t {0xE0B6})
            && rawFont.supportsCharacter(char32_t {0xF31B});
    };

    const auto installed = QFontDatabase::families();
    const auto available = std::ranges::any_of(
        installed,
        [&](const QString& family) {
            return QFontDatabase::isFixedPitch(family)
                && supportsTerminalSymbols(family);
        });
    if (!available) {
        QSKIP("No installed fixed-pitch Nerd Font can exercise fallback selection.");
    }

    InspectableTerminalView view(m_window.contentItem());
    const auto font =
        view.inputMethodQuery(Qt::ImFont).value<QFont>();
    QVERIFY(std::ranges::any_of(
        font.families(),
        supportsTerminalSymbols));
}

void TerminalKernelTest::checkpointPrecedesContiguousRawData()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        7,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto beforeCheckpoint = kernel.applyData({
        subscription,
        12,
        QByteArrayLiteral("rejected"),
    });
    QVERIFY(!beforeCheckpoint);
    QCOMPARE(
        beforeCheckpoint.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::MissingCheckpoint);

    const auto restored = kernel.installCheckpoint({
        subscription,
        12,
        3,
        20,
        checkpointFor(QByteArrayLiteral("checkpoint"), 20, 3),
    });
    QVERIFY(restored);
    QVERIFY(frameText(**restored).contains(QStringLiteral("checkpoint")));

    const auto continued = kernel.applyData({
        subscription,
        12,
        QByteArrayLiteral(" + raw"),
    });
    QVERIFY(continued);
    QCOMPARE((*continued)->nextSequence, 13);
    QVERIFY(frameText(**continued).contains(QStringLiteral("checkpoint + raw")));

    const auto duplicate = kernel.applyData({
        subscription,
        12,
        QByteArrayLiteral("duplicate"),
    });
    QVERIFY(!duplicate);
    QCOMPARE(
        duplicate.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::SequenceMismatch);
}

void TerminalKernelTest::rasterizerPreservesTerminalCellBackgrounds()
{
    kodosi::TerminalFrame frame;
    frame.columns = 2;
    frame.rows = 1;
    frame.background = QColor(QStringLiteral("#10131a"));
    frame.foreground = QColor(QStringLiteral("#f5f7ff"));
    frame.cells = {
        {
            .grapheme = QStringLiteral("A"),
            .foreground = frame.foreground,
            .background = frame.background,
        },
        {
            .foreground = frame.foreground,
            .background = QColor(QStringLiteral("#305080")),
        },
    };
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    const auto image = kodosi::TerminalRasterizer::render(frame, font, 1.0);

    QVERIFY(!image.isNull());
    const auto logicalSize = kodosi::TerminalRasterizer::logicalSize(frame, font);
    QCOMPARE(image.size(), logicalSize.toSize());
    const auto secondCellCenter = QPoint(
        static_cast<int>(logicalSize.width() * 0.75),
        static_cast<int>(logicalSize.height() * 0.5));
    QCOMPARE(image.pixelColor(secondCellCenter), QColor(QStringLiteral("#305080")));
}

void TerminalKernelTest::rasterizerUsesBrandedSelectionAndPreeditPalette()
{
    kodosi::TerminalFrame frame;
    frame.columns = 2;
    frame.rows = 1;
    frame.background = QColor(QStringLiteral("#0a0807"));
    frame.foreground = QColor(QStringLiteral("#f1e9e3"));
    frame.cells = {
        {
            .foreground = frame.foreground,
            .background = frame.background,
            .selected = true,
        },
        {
            .foreground = frame.foreground,
            .background = frame.background,
        },
    };
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    kodosi::TerminalRasterizer::Options options;
    options.palette.selectionBackground = QColor(QStringLiteral("#db8a62"));
    options.palette.selectionForeground = QColor(QStringLiteral("#160e0a"));
    options.palette.preeditBackground = QColor(QStringLiteral("#e69a72"));
    options.palette.preeditForeground = QColor(QStringLiteral("#160e0a"));
    options.overlay = {
        .preedit = QStringLiteral("x"),
        .column = 1,
        .row = 0,
    };

    const auto image =
        kodosi::TerminalRasterizer::render(frame, font, 1.0, options);
    QVERIFY(!image.isNull());
    QCOMPARE(
        image.pixelColor(QPoint(image.width() / 4, image.height() / 2)),
        options.palette.selectionBackground);
    QCOMPARE(
        image.pixelColor(QPoint(image.width() * 3 / 4, 1)),
        options.palette.preeditBackground);
}

void TerminalKernelTest::rasterizerHandlesWideTailBackgroundAndCursorSpan()
{
    kodosi::TerminalFrame frame;
    frame.columns = 2;
    frame.rows = 1;
    frame.background = QColor(QStringLiteral("#10131a"));
    frame.foreground = QColor(QStringLiteral("#f5f7ff"));
    frame.cursorColor = QColor(QStringLiteral("#ff405f"));
    frame.cells = {
        {
            .grapheme = QString::fromUtf8("界"),
            .foreground = frame.foreground,
            .background = QColor(QStringLiteral("#203040")),
            .width = 2,
        },
        {
            .foreground = frame.foreground,
            .background = QColor(QStringLiteral("#305080")),
            .width = 0,
        },
    };
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    const auto logicalSize = kodosi::TerminalRasterizer::logicalSize(frame, font);
    const auto withoutCursor = kodosi::TerminalRasterizer::render(frame, font, 1.0);
    const auto tailBackground = QPoint(
        static_cast<int>(logicalSize.width() * 0.75),
        static_cast<int>(logicalSize.height() - 2.0));
    QCOMPARE(
        withoutCursor.pixelColor(tailBackground),
        QColor(QStringLiteral("#305080")));

    frame.cursor = {
        .visible = true,
        .wideTail = true,
        .column = 1,
        .row = 0,
        .visualStyle = 1,
    };
    const auto withCursor = kodosi::TerminalRasterizer::render(frame, font, 1.0);
    const auto headCenter = QPoint(
        static_cast<int>(logicalSize.width() * 0.25),
        static_cast<int>(logicalSize.height() * 0.5));
    const auto tailCenter = QPoint(
        static_cast<int>(logicalSize.width() * 0.75),
        static_cast<int>(logicalSize.height() * 0.5));
    QVERIFY(withCursor.pixelColor(headCenter) != withoutCursor.pixelColor(headCenter));
    QVERIFY(withCursor.pixelColor(tailCenter) != withoutCursor.pixelColor(tailCenter));
}

void TerminalKernelTest::rasterizerAppliesLineHeightAndOwnedCursorPolicy()
{
    kodosi::TerminalFrame frame;
    frame.columns = 1;
    frame.rows = 1;
    frame.background = QColor(QStringLiteral("#101010"));
    frame.foreground = QColor(QStringLiteral("#eeeeee"));
    frame.cursorColor = QColor(QStringLiteral("#ff405f"));
    frame.cursor = {
        .visible = true,
        .column = 0,
        .row = 0,
        .visualStyle = 1,
    };
    frame.cells = {{
        .foreground = frame.foreground,
        .background = frame.background,
    }};
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);

    kodosi::TerminalRasterizer::Options hidden;
    hidden.cursorStyle = kodosi::TerminalCursorStyle::Bar;
    hidden.cursorPhaseVisible = false;
    const auto withoutCursor =
        kodosi::TerminalRasterizer::render(frame, font, 1.0, hidden);

    kodosi::TerminalRasterizer::Options bar;
    bar.cursorStyle = kodosi::TerminalCursorStyle::Bar;
    const auto withBar = kodosi::TerminalRasterizer::render(frame, font, 1.0, bar);
    QCOMPARE(withBar.pixelColor(QPoint(0, withBar.height() / 2)), frame.cursorColor);
    QCOMPARE(
        withoutCursor.pixelColor(QPoint(0, withoutCursor.height() / 2)),
        frame.background);

    kodosi::TerminalRasterizer::Options underline;
    underline.cursorStyle = kodosi::TerminalCursorStyle::Underline;
    underline.lineHeight = 1.5;
    const auto withUnderline =
        kodosi::TerminalRasterizer::render(frame, font, 1.0, underline);
    QVERIFY(withUnderline.height() > withBar.height());
    QCOMPARE(
        withUnderline.pixelColor(QPoint(withUnderline.width() / 2, withUnderline.height() - 1)),
        frame.cursorColor);
    QCOMPARE(
        withUnderline.pixelColor(QPoint(withUnderline.width() / 2, 0)),
        frame.background);
}

void TerminalKernelTest::rasterizerAlignsCellsToDevicePixels()
{
    kodosi::TerminalFrame frame;
    frame.columns = 7;
    frame.rows = 3;
    frame.background = QColor(QStringLiteral("#101010"));
    frame.foreground = QColor(QStringLiteral("#eeeeee"));
    frame.cells.fill({.foreground = frame.foreground, .background = frame.background}, 21);
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(13);
    for (const qreal ratio : {1.0, 1.25, 1.5, 1.75, 2.0}) {
        const auto cell = kodosi::TerminalRasterizer::cellSize(font, 1.1, ratio);
        QCOMPARE(std::round(cell.width() * ratio), cell.width() * ratio);
        QCOMPARE(std::round(cell.height() * ratio), cell.height() * ratio);
        const auto size = kodosi::TerminalRasterizer::logicalSize(frame, font, 1.1, ratio);
        kodosi::TerminalRasterizer::Options options;
        options.lineHeight = 1.1;
        const auto image = kodosi::TerminalRasterizer::render(frame, font, ratio, options);
        QCOMPARE(image.size(), (size * ratio).toSize());
    }
}

void TerminalKernelTest::rasterizerFillsBlockAndPowerlineCells()
{
    const QColor segment(QStringLiteral("#f5b342"));
    kodosi::TerminalFrame frame;
    frame.columns = 3;
    frame.rows = 3;
    frame.background = QColor(QStringLiteral("#000000"));
    frame.foreground = QColor(QStringLiteral("#ffffff"));
    frame.cells.fill({.foreground = frame.foreground, .background = frame.background}, 9);
    for (const auto [column, codepoint] : {std::pair {0, 0xE0B6}, std::pair {1, 0x2588}, std::pair {2, 0xE0B0}}) {
        frame.cells[3 + column] = {
            .grapheme = QString(QChar(codepoint)),
            .foreground = segment,
            .background = frame.background,
        };
    }
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    for (const qreal ratio : {1.0, 1.5}) {
        const auto image = kodosi::TerminalRasterizer::render(frame, font, ratio);
        const auto cell = kodosi::TerminalRasterizer::cellSize(font, 1.0, ratio) * ratio;
        const auto width = qRound(cell.width());
        const auto height = qRound(cell.height());
        const auto pixel = [&image](const int x, const int y) { return image.pixelColor(x, y); };

        QCOMPARE(pixel(width, height), segment);
        QCOMPARE(pixel((2 * width) - 1, (2 * height) - 1), segment);
        QCOMPARE(pixel(2 * width, height + (height / 2)), segment);
        QCOMPARE(pixel(width - 1, height + (height / 2)), segment);
        QCOMPARE(pixel((3 * width) - 1, height), frame.background);
        QCOMPARE(pixel(0, height), frame.background);
        for (int x = 0; x < image.width(); ++x) {
            QCOMPARE(pixel(x, height - 1), frame.background);
            QCOMPARE(pixel(x, 2 * height), frame.background);
        }
    }
}

void TerminalKernelTest::rasterizerDrawsImagesInTheirLayers()
{
    using Layer = kodosi::TerminalImagePlacement::Layer;
    const auto filled = [](const QColor& color) {
        QImage image(4, 4, QImage::Format_RGBX8888);
        image.fill(color);
        return image;
    };
    kodosi::TerminalFrame frame;
    frame.columns = 4;
    frame.rows = 1;
    frame.background = QColor(QStringLiteral("#000000"));
    frame.foreground = QColor(QStringLiteral("#ffffff"));
    frame.cells = {
        {.foreground = frame.foreground, .background = QColor(QStringLiteral("#0000ff"))},
        {.foreground = frame.foreground, .background = frame.background},
        {
            .grapheme = QStringLiteral("\u2588"),
            .foreground = frame.foreground,
            .background = frame.background,
        },
        {
            .grapheme = QString::fromUcs4(U"\U0010EEEE"),
            .foreground = frame.foreground,
            .background = frame.background,
        },
    };
    frame.images = {
        {
            .pixels = filled(Qt::red),
            .source = QRect(0, 0, 4, 4),
            .bounds = QRectF(0, 0, 2, 1),
            .layer = Layer::BelowBackground,
        },
        {
            .pixels = filled(Qt::green),
            .source = QRect(0, 0, 4, 4),
            .bounds = QRectF(2, 0, 1, 1),
            .layer = Layer::AboveText,
        },
    };
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    const auto image = kodosi::TerminalRasterizer::render(frame, font, 1.0);
    const auto cell = kodosi::TerminalRasterizer::cellSize(font);
    const auto center = [&](const int column) {
        return image.pixelColor(QPoint(
            static_cast<int>(cell.width() * (column + 0.5)),
            static_cast<int>(cell.height() * 0.5)));
    };

    QCOMPARE(center(0), QColor(QStringLiteral("#0000ff")));
    QCOMPARE(center(1), QColor(Qt::red));
    QCOMPARE(center(2), QColor(Qt::green));
    for (auto y = 0; y < static_cast<int>(cell.height()); ++y) {
        for (auto x = static_cast<int>(std::ceil(cell.width() * 3)); x < image.width(); ++x) {
            QCOMPARE(image.pixelColor(x, y), frame.background);
        }
    }
}

void TerminalKernelTest::rasterizerPaintsChangedRowsAsTheWholeFrame()
{
    using Options = kodosi::TerminalRasterizer::Options;
    kodosi::TerminalFrame earlier;
    earlier.columns = 12;
    earlier.rows = 10;
    earlier.background = QColor(QStringLiteral("#120f0d"));
    earlier.foreground = QColor(QStringLiteral("#dcdcdc"));
    earlier.cursorColor = QColor(QStringLiteral("#db8a62"));
    earlier.cursor = {.visible = true, .column = 2, .row = 1, .visualStyle = 1};
    earlier.cells.resize(
        static_cast<qsizetype>(earlier.columns) * earlier.rows,
        {.foreground = earlier.foreground, .background = earlier.background});
    const auto write = [](kodosi::TerminalFrame& frame, const int row, const QString& text) {
        for (qsizetype column = 0; column < text.size(); ++column) {
            auto& cell = frame.cells[row * frame.columns + column];
            cell.grapheme = text.mid(column, 1);
            cell.underline = row % 2;
            cell.selected = row == 4;
        }
    };
    write(earlier, 0, QStringLiteral("$ ls gyjpq"));
    write(earlier, 2, QStringLiteral("█ Agy"));
    write(earlier, 4, QStringLiteral("selected"));
    QImage pixels(8, 8, QImage::Format_RGBA8888_Premultiplied);
    pixels.fill(Qt::red);
    earlier.images = {{
        .pixels = pixels,
        .source = QRect(0, 0, 8, 8),
        .bounds = QRectF(8.0, 1.5, 3.0, 2.0),
        .layer = kodosi::TerminalImagePlacement::Layer::BelowText,
    }};

    auto typed = earlier;
    write(typed, 3, QStringLiteral("typed"));
    typed.cursor.row = 3;
    typed.cursor.column = 5;
    auto moved = earlier;
    moved.images[0].bounds = QRectF(7.0, 3.25, 3.0, 2.0);
    auto scrolled = earlier;
    std::copy(
        earlier.cells.cbegin() + earlier.columns,
        earlier.cells.cend(),
        scrolled.cells.begin());
    std::fill(
        scrolled.cells.end() - earlier.columns,
        scrolled.cells.end(),
        kodosi::TerminalCell {.foreground = earlier.foreground, .background = earlier.background});
    write(scrolled, earlier.rows - 1, QStringLiteral("new line gy"));
    scrolled.images[0].bounds.translate(0.0, -1.0);
    Options blink;
    blink.cursorPhaseVisible = false;
    Options composing;
    composing.overlay = {QStringLiteral("ab"), 2, 1};
    struct Change {
        const kodosi::TerminalFrame* frame;
        Options options;
        int mostRows;
    };
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    for (const auto lineHeight : {0.8, 1.2}) {
        for (const auto ratio : {1.0, 1.5}) {
            Options before;
            before.lineHeight = lineHeight;
            for (auto [frame, options, mostRows] : {
                     Change {&earlier, Options {}, 0},
                     Change {&typed, Options {}, 4},
                     Change {&earlier, blink, 1},
                     Change {&moved, Options {}, 5},
                     Change {&scrolled, Options {}, 4},
                     Change {&earlier, composing, 3},
                 }) {
                options.lineHeight = lineHeight;
                auto image = kodosi::TerminalRasterizer::render(earlier, font, ratio, before);
                const auto painted = kodosi::TerminalRasterizer::update(
                    image, earlier, before, *frame, font, options);
                QVERIFY(painted.has_value());
                QVERIFY(*painted <= mostRows);
                QCOMPARE(image, kodosi::TerminalRasterizer::render(*frame, font, ratio, options));
            }
        }
    }

    auto recolored = earlier;
    recolored.background = QColor(QStringLiteral("#ffffff"));
    auto image = kodosi::TerminalRasterizer::render(earlier, font, 1.0, Options {});
    QVERIFY(!kodosi::TerminalRasterizer::update(
        image, earlier, Options {}, recolored, font, Options {}));
}

void TerminalKernelTest::rasterizerDrawsARowAlikeInEveryRow()
{
    kodosi::TerminalFrame frame;
    frame.columns = 8;
    frame.rows = 9;
    frame.background = QColor(QStringLiteral("#120f0d"));
    frame.foreground = QColor(QStringLiteral("#dcdcdc"));
    frame.cells.resize(
        static_cast<qsizetype>(frame.columns) * frame.rows,
        {.foreground = frame.foreground, .background = frame.background});
    const auto line = QStringLiteral("▀▄▌▐▖▝█g");
    for (qsizetype index = 0; index < frame.cells.size(); ++index) {
        auto& cell = frame.cells[index];
        cell.grapheme = line.mid(index % frame.columns, 1);
        cell.underline = cell.grapheme == u"g" ? 1 : 0;
        cell.strikethrough = cell.grapheme == u"g";
    }
    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    for (const auto ratio : {1.25, 1.5, 1.75, 2.25}) {
        for (const auto lineHeight : {1.0, 1.3, 1.6}) {
            kodosi::TerminalRasterizer::Options options;
            options.lineHeight = lineHeight;
            const auto image = kodosi::TerminalRasterizer::render(frame, font, ratio, options);
            const auto rowHeight = image.height() / frame.rows;
            QCOMPARE(rowHeight * frame.rows, image.height());
            const auto second = image.copy(0, rowHeight, image.width(), rowHeight);
            for (int row = 2; row + 1 < frame.rows; ++row) {
                QCOMPARE(image.copy(0, rowHeight * row, image.width(), rowHeight), second);
            }
        }
    }
}

void TerminalKernelTest::kernelAppliesNativeCursorDefaultsAndScrollback()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        13,
    };
    kodosi::GhosttyTerminalKernel kernel({
        .cursorStyle = kodosi::TerminalCursorStyle::Bar,
        .cursorBlink = true,
        .scrollbackLines = 12'000,
        .scrollbackBytes = kodosi::terminalScrollbackByteBudget(12'000),
    });
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        2,
        12,
        checkpointFor(QByteArrayLiteral("ready"), 12, 2),
    }));
    const auto limit = kernel.scrollbackLimitLines();
    QVERIFY(limit);
    QCOMPARE(*limit, std::size_t {12'000});
    const auto byteLimit = kernel.scrollbackLimitBytes();
    QVERIFY(byteLimit);
    QCOMPARE(
        *byteLimit,
        kodosi::terminalScrollbackByteBudget(12'000));

    const auto bar = kernel.applyData({
        subscription,
        1,
        QByteArrayLiteral("\x1b[0 q"),
    });
    QVERIFY(bar);
    QCOMPARE((*bar)->cursor.visualStyle, 0);
    QVERIFY((*bar)->cursor.blinking);

    QVERIFY(kernel.configure({
        .cursorStyle = kodosi::TerminalCursorStyle::Underline,
        .cursorBlink = false,
        .scrollbackLines = 25'000,
        .scrollbackBytes = kodosi::terminalScrollbackByteBudget(25'000),
    }));
    const auto updatedLimit = kernel.scrollbackLimitLines();
    QVERIFY(updatedLimit);
    QCOMPARE(*updatedLimit, std::size_t {25'000});
    const auto updatedByteLimit = kernel.scrollbackLimitBytes();
    QVERIFY(updatedByteLimit);
    QCOMPARE(
        *updatedByteLimit,
        kodosi::terminalScrollbackByteBudget(25'000));
    const auto underline = kernel.applyData({
        subscription,
        2,
        QByteArrayLiteral("\x1b[0 q"),
    });
    QVERIFY(underline);
    QCOMPARE((*underline)->cursor.visualStyle, 2);
    QVERIFY(!(*underline)->cursor.blinking);
}

void TerminalKernelTest::scrollbackReconfigurationPublishesReplacementFrame()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("configure"),
        QStringLiteral("configure-subscription"),
        14,
    };
    QVector<kodosi::GhosttyTerminalKernel::Frame> frames;
    std::optional<kodosi::GhosttyTerminalKernel::Failure> failure;
    kodosi::TerminalSessionRegistry registry;
    QVERIFY(registry.registerSurface(
        {subscription, 1},
        {
            .frameChanged = [&](auto frame) { frames.append(std::move(frame)); },
            .failed = [&](auto value) { failure = std::move(value); },
        }));

    QByteArray output;
    for (int line = 0; line < 4'000; ++line) {
        output += QByteArrayLiteral("history-");
        output += QByteArray::number(line).rightJustified(4, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        5,
        20,
        checkpointFor(output, 20, 5),
    }));
    QVERIFY(registry.scrollViewport(kodosi::TerminalSurfaceIdentity {subscription, 1}, -100'000));
    const auto topFrame = frames.constLast();
    QVERIFY(topFrame->scroll.viewportOffset == 0);
    QVERIFY(registry.beginSelection(
        kodosi::TerminalSurfaceIdentity {subscription, 1},
        topFrame->viewportRevision,
        0,
        0));
    QVERIFY(registry.updateSelection(
        kodosi::TerminalSurfaceIdentity {subscription, 1},
        topFrame->viewportRevision,
        3,
        0));
    const auto selectedBeforeConfigure = registry.selectedText(kodosi::TerminalSurfaceIdentity {subscription, 1});
    QVERIFY(selectedBeforeConfigure);
    QVERIFY(!selectedBeforeConfigure->isEmpty());

    const auto publishedBeforeConfigure = frames.size();
    const auto frameBeforeConfigure = frames.constLast();
    const kodosi::TerminalKernelSettings reduced {
        .scrollbackLines = 100,
        .scrollbackBytes = kodosi::terminalScrollbackByteBudget(100),
    };
    QVERIFY(registry.configure(kodosi::TerminalSurfaceIdentity {subscription, 1}, reduced));
    QVERIFY(!failure.has_value());
    QCOMPARE(frames.size(), publishedBeforeConfigure + 1);
    const auto configuredFrame = frames.constLast();
    QCOMPARE(
        configuredFrame->viewportRevision,
        frameBeforeConfigure->viewportRevision + 1);
    QCOMPARE(
        configuredFrame->displayRevision,
        frameBeforeConfigure->displayRevision + 1);
    QVERIFY(configuredFrame != frameBeforeConfigure);
    QVERIFY(configuredFrame->scroll.totalRows < frameBeforeConfigure->scroll.totalRows);
    const auto selectedAfterConfigure = registry.selectedText(kodosi::TerminalSurfaceIdentity {subscription, 1});
    QVERIFY(selectedAfterConfigure);
    QVERIFY(selectedAfterConfigure->isEmpty());

    QVERIFY(registry.configure(kodosi::TerminalSurfaceIdentity {subscription, 1}, reduced));
    QCOMPARE(frames.size(), publishedBeforeConfigure + 1);
}

void TerminalKernelTest::terminalViewMailboxRejectsReversedDisplayRevisions()
{
    kodosi::detail::TerminalFrameMailbox mailbox;
    mailbox.reset(41);

    auto revision10 = std::make_shared<kodosi::TerminalFrame>();
    revision10->displayRevision = 10;
    const auto first = mailbox.enqueue(41, revision10);
    QVERIFY(first.accepted);
    QVERIFY(first.queueDrain);

    auto revision12 = std::make_shared<kodosi::TerminalFrame>();
    revision12->displayRevision = 12;
    const auto coalesced = mailbox.enqueue(41, revision12);
    QVERIFY(coalesced.accepted);
    QVERIFY(!coalesced.queueDrain);

    auto revision11 = std::make_shared<kodosi::TerminalFrame>();
    revision11->displayRevision = 11;
    const auto reversedPending = mailbox.enqueue(41, revision11);
    QVERIFY(!reversedPending.accepted);
    QCOMPARE(mailbox.take(41), revision12);

    const auto reversedPresented = mailbox.enqueue(41, revision11);
    QVERIFY(!reversedPresented.accepted);

    mailbox.reset(42);
    auto nextIncarnation = std::make_shared<kodosi::TerminalFrame>();
    nextIncarnation->displayRevision = 1;
    const auto resetRevision = mailbox.enqueue(42, nextIncarnation);
    QVERIFY(resetRevision.accepted);
    QVERIFY(resetRevision.queueDrain);
    QVERIFY(!mailbox.enqueue(41, revision12).accepted);
    QCOMPARE(mailbox.take(42), nextIncarnation);
}

void TerminalKernelTest::terminalViewOwnsValidatedDisplaySettings()
{
    kodosi::TerminalView view(m_window.contentItem());
    view.setFontFamily(QStringLiteral("Iosevka"));
    view.setFontPixelSize(20);
    view.setLineHeight(1.6);
    view.setCursorStyle(2);
    view.setCursorBlink(true);
    view.setScrollbackLines(40'000);

    QCOMPARE(view.fontFamily(), QStringLiteral("Iosevka"));
    QCOMPARE(view.fontPixelSize(), 20);
    QCOMPARE(view.lineHeight(), 1.6);
    QCOMPARE(view.cursorStyle(), 2);
    QVERIFY(view.cursorBlink());
    QCOMPARE(view.scrollbackLines(), 40'000);

    auto* timer = view.findChild<QTimer*>(
        QStringLiteral("terminal.cursorBlinkTimer"));
    QVERIFY(timer != nullptr);
    QVERIFY(!timer->isActive());
}

void TerminalKernelTest::terminalViewOwnsExplicitSelectionAndPreeditPalette()
{
    kodosi::TerminalView view(m_window.contentItem());
    const auto selectionBackground = QColor(QStringLiteral("#db8a62"));
    const auto selectionForeground = QColor(QStringLiteral("#160e0a"));
    const auto preeditBackground = QColor(QStringLiteral("#e69a72"));
    const auto preeditForeground = QColor(QStringLiteral("#21100e"));

    view.setSelectionBackground(selectionBackground);
    view.setSelectionForeground(selectionForeground);
    view.setPreeditBackground(preeditBackground);
    view.setPreeditForeground(preeditForeground);

    QCOMPARE(view.selectionBackground(), selectionBackground);
    QCOMPARE(view.selectionForeground(), selectionForeground);
    QCOMPARE(view.preeditBackground(), preeditBackground);
    QCOMPARE(view.preeditForeground(), preeditForeground);
}

void TerminalKernelTest::keyEncodingUsesRestoredTerminalModes()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        9,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        3,
        2,
        12,
        checkpointFor(QByteArrayLiteral("\x1b[?1h"), 12, 2),
    }));

    const auto arrow = kernel.encodeKey(
        subscription,
        {
            .key = kodosi::TerminalKey::ArrowUp,
            .action = kodosi::TerminalKeyAction::Press,
        });
    QVERIFY(arrow);
    QCOMPARE(*arrow, QByteArrayLiteral("\x1bOA"));

    const auto interrupt = kernel.encodeKey(
        subscription,
        {
            .key = kodosi::TerminalKey::Character,
            .action = kodosi::TerminalKeyAction::Press,
            .text = QStringLiteral("c"),
            .unshiftedCodepoint = 'c',
            .modifiers = {.control = true},
        });
    QVERIFY(interrupt);
    QCOMPARE(*interrupt, QByteArrayLiteral("\x03"));

    const auto legacyRelease = kernel.encodeKey(
        subscription,
        {
            .key = kodosi::TerminalKey::ArrowUp,
            .action = kodosi::TerminalKeyAction::Release,
        });
    QVERIFY(legacyRelease);
    QVERIFY(legacyRelease->isEmpty());
}

void TerminalKernelTest::mouseEncodingUsesRestoredTerminalModes()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        1,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        2,
        12,
        checkpointFor(
            QByteArrayLiteral("\x1b[?1000h\x1b[?1006h"),
            12,
            2),
    }));

    const kodosi::TerminalMouseEvent press {
        .action = kodosi::TerminalMouseAction::Press,
        .button = kodosi::TerminalMouseButton::Left,
        .modifiers = {},
        .x = 4,
        .y = 8,
        .screenWidth = 96,
        .screenHeight = 32,
        .cellWidth = 8,
        .cellHeight = 16,
        .anyButtonPressed = true,
    };
    const auto encodedPress = kernel.encodeMouse(subscription, press);
    QVERIFY(encodedPress);
    QCOMPARE(*encodedPress, QByteArrayLiteral("\x1b[<0;1;1M"));

    auto release = press;
    release.action = kodosi::TerminalMouseAction::Release;
    release.anyButtonPressed = false;
    const auto encodedRelease = kernel.encodeMouse(subscription, release);
    QVERIFY(encodedRelease);
    QCOMPARE(*encodedRelease, QByteArrayLiteral("\x1b[<0;1;1m"));
}

void TerminalKernelTest::terminalViewRoutesTrackedMouseToPty()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("mouse"),
        QStringLiteral("mouse-subscription"),
        1,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, true);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("mouse-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(
            QByteArrayLiteral("\x1b[?1000h\x1b[?1006h"),
            40,
            4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF point(cell.width() * 0.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        point,
        point,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        point,
        point,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);

    QCOMPARE(
        dispatcher.inputCommands,
        QVector<QByteArray>({
            QByteArrayLiteral("\x1b[<0;1;1M"),
            QByteArrayLiteral("\x1b[<0;1;1m"),
        }));
}

void TerminalKernelTest::remoteGridFitsAndMapsAccessibleMouseCoordinates()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(400, 220);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(400, 220));
    view.setTerminalInteraction(true, true);
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("remote"), QStringLiteral("remote-view"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("remote-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription, 1, 24, 120,
        checkpointFor(QByteArrayLiteral(
            "\x1b[?1000h\x1b[?1006hLEFT\x1b[1;116HRIGHT\x1b[24;114HBOTTOM"), 120, 24),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    window.show();
    QTRY_VERIFY(view.terminalReady());
    const auto text = view.accessibleText();
    const auto right = static_cast<int>(text.indexOf(QStringLiteral("RIGHT")));
    const auto bottom = static_cast<int>(text.indexOf(QStringLiteral("BOTTOM")));
    QVERIFY(right >= 0 && bottom >= 0);
    QVERIFY(view.viewportScale() < 1);
    const QRect pane(window.mapToGlobal(QPoint(0, 0)), window.size());
    QVERIFY(pane.contains(view.accessibleCharacterRect(0).center()));
    QVERIFY(pane.contains(view.accessibleCharacterRect(right + 4).center()));
    QVERIFY(pane.contains(view.accessibleCharacterRect(bottom + 5).center()));
    QCOMPARE(view.accessibleOffsetAt(view.accessibleCharacterRect(right).center()), right);

    const auto click = [&](const int offset) {
        const QPointF point = window.mapFromGlobal(view.accessibleCharacterRect(offset).center());
        QMouseEvent press(QEvent::MouseButtonPress, point, point,
            Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QMouseEvent release(QEvent::MouseButtonRelease, point, point,
            Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(&view, &press);
        QCoreApplication::sendEvent(&view, &release);
    };
    click(right);
    QCOMPARE(dispatcher.inputCommands.constLast(), QByteArrayLiteral("\x1b[<0;116;1m"));
    QVERIFY(view.canResize());
    QVERIFY(view.viewportScale() < 1);
    const auto rejected = dispatcher.terminalCommands.last();
    auto outcome = rejected;
    outcome.insert(QStringLiteral("type"), QStringLiteral("term.resizeRejected"));
    outcome.insert(QStringLiteral("reason"), QStringLiteral("Another controller owns size"));
    registry.receiveControl({subscription, QJsonDocument(outcome).toJson(QJsonDocument::Compact)});
    QCoreApplication::processEvents();
    QVERIFY(view.canResize());
    QVERIFY(view.viewportScale() < 1);
}

void TerminalKernelTest::pasteEncodingUsesRestoredTerminalModes()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        10,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        3,
        2,
        12,
        checkpointFor(QByteArrayLiteral("\x1b[?2004h"), 12, 2),
    }));

    const auto bracketed = kernel.encodePaste(
        subscription,
        QByteArrayLiteral("first\nsecond"));
    QVERIFY(bracketed);
    QCOMPARE(
        *bracketed,
        QByteArrayLiteral("\x1b[200~first\nsecond\x1b[201~"));

    QVERIFY(kernel.applyData({
        subscription,
        3,
        QByteArrayLiteral("\x1b[?2004l"),
    }));
    const auto plain = kernel.encodePaste(
        subscription,
        QByteArrayLiteral("single line"));
    QVERIFY(plain);
    QCOMPARE(*plain, QByteArrayLiteral("single line"));

    const auto multiline = kernel.encodePaste(
        subscription,
        QByteArrayLiteral("first\nsecond"));
    QVERIFY(multiline);
    QCOMPARE(*multiline, QByteArrayLiteral("first\rsecond"));

    const auto control = kernel.encodePaste(
        subscription,
        QByteArrayLiteral("echo \x03"));
    QVERIFY(control);
    QCOMPARE(*control, QByteArrayLiteral("echo  "));
}

void TerminalKernelTest::wheelScrollsGhosttyViewportWithoutChangingSequence()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("scroll"),
        QStringLiteral("scroll-subscription"),
        12,
    };
    QByteArray output;
    for (int line = 0; line < 40; ++line) {
        output += QByteArrayLiteral("line-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    kodosi::GhosttyTerminalKernel kernel;
    const auto initial = kernel.installCheckpoint({
        subscription,
        17,
        8,
        20,
        checkpointFor(output, 20, 8),
    });
    QVERIFY(initial);
    QVERIFY(frameText(**initial).contains(QStringLiteral("line-39")));
    QVERIFY(!frameText(**initial).contains(QStringLiteral("line-00")));
    QCOMPARE((*initial)->scroll.viewportOffset + (*initial)->scroll.viewportRows,
             (*initial)->scroll.totalRows);

    const auto scrolled = kernel.scrollViewport(subscription, -100);
    QVERIFY(scrolled);
    QCOMPARE((*scrolled)->nextSequence, 17);
    QCOMPARE((*scrolled)->scroll.viewportOffset, 0);
    QVERIFY(frameText(**scrolled).contains(QStringLiteral("line-00")));

    const auto stale = kernel.scrollViewport(
        {
            subscription.sessionId,
            subscription.subscriptionId,
            subscription.generation + 1,
        },
        1);
    QVERIFY(!stale);
    QCOMPARE(
        stale.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::StaleSubscription);
}

void TerminalKernelTest::selectionUsesGhosttyTrackedStateAndFormatting()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        11,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto initial = kernel.installCheckpoint({
        subscription,
        1,
        2,
        20,
        checkpointFor(QByteArrayLiteral("hello world"), 20, 2),
    });
    QVERIFY(initial);

    QVERIFY(kernel.beginSelection(
        subscription,
        (*initial)->viewportRevision,
        0,
        0));
    const auto selected = kernel.updateSelection(
        subscription,
        (*initial)->viewportRevision,
        4,
        0);
    QVERIFY(selected);
    for (std::uint16_t column = 0; column <= 4; ++column) {
        QVERIFY((*selected)->cell(column, 0)->selected);
    }
    const auto text = kernel.selectedText(subscription);
    QVERIFY(text);
    QCOMPARE(*text, QStringLiteral("hello"));

    const auto cleared = kernel.clearSelection(subscription);
    QVERIFY(cleared);
    QVERIFY(!(*cleared)->cell(0, 0)->selected);
}

void TerminalKernelTest::aSelectionShowsOnlyAfterThePointerLeavesItsFirstCell()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        11,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto initial = kernel.installCheckpoint({
        subscription,
        1,
        2,
        20,
        checkpointFor(QByteArrayLiteral("hello world"), 20, 2),
    });
    QVERIFY(initial);
    const auto pressed = kernel.beginSelection(subscription, (*initial)->viewportRevision, 2, 0);
    QVERIFY(pressed);
    QVERIFY(!(*pressed)->cell(2, 0)->selected);
    QVERIFY(kernel.updateSelection(subscription, (*initial)->viewportRevision, 2, 0));
    QCOMPARE(*kernel.selectedText(subscription), QString());

    const auto dragged = kernel.updateSelection(subscription, (*initial)->viewportRevision, 4, 0);
    QVERIFY(dragged);
    QCOMPARE(*kernel.selectedText(subscription), QStringLiteral("llo"));
    const auto back = kernel.updateSelection(subscription, (*initial)->viewportRevision, 2, 0);
    QVERIFY(back);
    QVERIFY((*back)->cell(2, 0)->selected);
    QCOMPARE(*kernel.selectedText(subscription), QStringLiteral("l"));
}

void TerminalKernelTest::framesDrawTheKodosiTerminalColors()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        11,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto frame = kernel.installCheckpoint({
        subscription,
        1,
        2,
        20,
        checkpointFor(QByteArrayLiteral("\x1b[31mred"), 20, 2),
    });
    QVERIFY(frame);
    QCOMPARE((*frame)->background, QColor(0x0d, 0x0b, 0x09));
    QCOMPARE((*frame)->foreground, QColor(0xe8, 0xe1, 0xd9));
    QCOMPARE((*frame)->cursorColor, QColor(0xcc, 0x7a, 0x45));
    QCOMPARE((*frame)->cell(0, 0)->foreground, QColor(0xc4, 0x40, 0x40));
}

void TerminalKernelTest::aWebAddressInTheTextIsALinkThatEndsBeforePunctuation()
{
    kodosi::TerminalFrame frame;
    frame.columns = 16;
    frame.rows = 3;
    const QString text = QStringLiteral("see https://exam"
                                        "ple.com/a_(b)). "
                                        "plain text here ");
    for (const auto character : text) {
        frame.cells.push_back({.grapheme = character == u' ' ? QString() : QString(character)});
    }
    frame.wrappedRows = {true, false, false};
    const QUrl expected(QStringLiteral("https://example.com/a_(b)"));
    QCOMPARE(kodosi::terminalTextLinkAt(frame, 6, 0), expected);
    QCOMPARE(kodosi::terminalTextLinkAt(frame, 3, 1), expected);
    QVERIFY(!kodosi::terminalTextLinkAt(frame, 13, 1));
    QVERIFY(!kodosi::terminalTextLinkAt(frame, 1, 0));
    QVERIFY(!kodosi::terminalTextLinkAt(frame, 2, 2));
    frame.wrappedRows = {false, false, false};
    QVERIFY(!kodosi::terminalTextLinkAt(frame, 3, 1));
}

void TerminalKernelTest::scrolledSelectionUsesVisibleViewportCoordinates()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("selection-scroll"),
        QStringLiteral("selection-scroll-subscription"),
        13,
    };
    QByteArray output;
    for (int line = 0; line < 20; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        5,
        12,
        checkpointFor(output, 12, 5),
    }));
    const auto scrolled = kernel.scrollViewport(subscription, -100);
    QVERIFY(scrolled);
    QVERIFY(frameText(**scrolled).startsWith(QStringLiteral("row-00")));

    QVERIFY(kernel.beginSelection(
        subscription,
        (*scrolled)->viewportRevision,
        0,
        0));
    const auto selected = kernel.updateSelection(
        subscription,
        (*scrolled)->viewportRevision,
        5,
        0);
    QVERIFY(selected);
    const auto text = kernel.selectedText(subscription);
    QVERIFY(text);
    QCOMPARE(*text, QStringLiteral("row-00"));
}

void TerminalKernelTest::selectionEndpointsTrackIncomingOutput()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("selection-output"),
        QStringLiteral("selection-output-subscription"),
        14,
    };
    QByteArray output;
    for (int line = 0; line < 20; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    kodosi::GhosttyTerminalKernel kernel;
    const auto initial = kernel.installCheckpoint({
        subscription,
        1,
        5,
        12,
        checkpointFor(output, 12, 5),
    });
    QVERIFY(initial);
    const auto anchorText = frameText(**initial).left(6);
    QVERIFY(anchorText.startsWith(QStringLiteral("row-")));
    QVERIFY(kernel.beginSelection(
        subscription,
        (*initial)->viewportRevision,
        0,
        0));

    const auto advanced = kernel.applyData({
        subscription,
        1,
        QByteArrayLiteral("row-20\r\n"),
    });
    QVERIFY(advanced);
    const auto endpointText = frameText(**advanced).left(6);
    QVERIFY(endpointText.startsWith(QStringLiteral("row-")));
    QVERIFY(endpointText != anchorText);
    QVERIFY(kernel.updateSelection(
        subscription,
        (*advanced)->viewportRevision,
        5,
        0));

    const auto text = kernel.selectedText(subscription);
    QVERIFY(text);
    QVERIFY(text->contains(anchorText));
    QVERIFY(text->contains(endpointText));
}

void TerminalKernelTest::terminalViewExposesNativeAccessibleTextInterface()
{
    kodosi::installTerminalAccessibility();
    kodosi::TerminalView view(m_window.contentItem());
    auto* accessible = QAccessible::queryAccessibleInterface(&view);

    QVERIFY(accessible != nullptr);
    QCOMPARE(accessible->role(), QAccessible::Terminal);
    QVERIFY(accessible->textInterface() != nullptr);
    QCOMPARE(accessible->text(QAccessible::Name), QStringLiteral("Terminal"));
    QVERIFY(accessible->state().disabled);
    QVERIFY(accessible->state().readOnly);
    QVERIFY(!accessible->state().editable);

    view.setTerminalInteraction(true, true);
    QVERIFY(!accessible->state().readOnly);
    QVERIFY(accessible->state().editable);
}

void TerminalKernelTest::qmlConstructionEnrollsTerminalInAccessibleTree()
{
    qmlRegisterType<kodosi::TerminalView>(
        "Kodosi.Terminal.Test",
        1,
        0,
        "TerminalView");
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(
        QByteArrayLiteral(
            "import QtQuick\nimport Kodosi.Terminal.Test\nTerminalView {}"),
        {});
    std::unique_ptr<QObject> view(component.create());

    QVERIFY2(view != nullptr, qPrintable(component.errorString()));
    QObject* attached = nullptr;
    for (auto* child : view->children()) {
        if (QByteArrayView(child->metaObject()->className())
            .contains(QByteArrayView("QQuickAccessibleAttached"))) {
            attached = child;
            break;
        }
    }
    QVERIFY(attached != nullptr);
    QCOMPARE(
        attached->property(QByteArrayLiteral("role")).toInt(),
        static_cast<int>(QAccessible::Terminal));
    QCOMPARE(
        attached->property(QByteArrayLiteral("name")).toString(),
        QStringLiteral("Terminal"));
}

void TerminalKernelTest::terminalViewGatesDeniedCommandsAndRevocation()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        7,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);

    QKeyEvent deniedPress(
        QEvent::KeyPress,
        Qt::Key_A,
        Qt::NoModifier,
        QStringLiteral("a"));
    QCoreApplication::sendEvent(&view, &deniedPress);
    QKeyEvent deniedRelease(
        QEvent::KeyRelease,
        Qt::Key_A,
        Qt::NoModifier,
        QStringLiteral("a"));
    QCoreApplication::sendEvent(&view, &deniedRelease);
    QInputMethodEvent deniedIme;
    deniedIme.setCommitString(QStringLiteral("denied"));
    QCoreApplication::sendEvent(&view, &deniedIme);
    QFocusEvent deniedFocus(QEvent::FocusIn);
    QCoreApplication::sendEvent(&view, &deniedFocus);
    QCoreApplication::processEvents();

    QCOMPARE(dispatcher.inputCommands.size(), 0);
    QCOMPARE(dispatcher.terminalCommands.size(), 0);

    view.setTerminalInteraction(true, false);
    QCOMPARE(dispatcher.terminalCommands.size(), 1);
    QCOMPARE(
        dispatcher.terminalCommands.constLast()
            .value(QStringLiteral("type"))
            .toString(),
        QStringLiteral("session.focus"));
    dispatcher.busyInputCommands = 1;
    QKeyEvent acceptedPress(
        QEvent::KeyPress,
        Qt::Key_A,
        Qt::NoModifier,
        QStringLiteral("a"));
    QCoreApplication::sendEvent(&view, &acceptedPress);
    QFocusEvent acceptedFocus(QEvent::FocusIn);
    QCoreApplication::sendEvent(&view, &acceptedFocus);
    QVERIFY(!dispatcher.inputCommands.isEmpty());
    QCOMPARE(
        dispatcher.terminalCommands.constLast()
            .value(QStringLiteral("type"))
            .toString(),
        QStringLiteral("session.focus"));

    const auto inputAttempts = dispatcher.inputCommands.size();
    view.setTerminalInteraction(false, false);
    QCOMPARE(
        dispatcher.terminalCommands.constLast()
            .value(QStringLiteral("type"))
            .toString(),
        QStringLiteral("session.blur"));
    QVERIFY(view.readOnly());
    QVERIFY(!view.canSendInput());
    view.setTerminalInteraction(true, false);
    QCOMPARE(
        dispatcher.terminalCommands.constLast()
            .value(QStringLiteral("type"))
            .toString(),
        QStringLiteral("session.focus"));
    QCOMPARE(dispatcher.terminalCommands.size(), 3);
    QTest::qWait(40);
    QCOMPARE(dispatcher.inputCommands.size(), inputAttempts);
    QVERIFY(!view.readOnly());
    QVERIFY(view.canSendInput());
}

void TerminalKernelTest::terminalViewJoinsSurrogateHalvesFromSeparateKeyEvents()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("surrogates"),
        QStringLiteral("subscription"),
        8,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);

    const auto emoji = QString::fromUcs4(U"\U0001F600");
    const auto press = [&view](const QString& text) {
        QKeyEvent event(QEvent::KeyPress, 0, Qt::NoModifier, text);
        QCoreApplication::sendEvent(&view, &event);
    };
    press(emoji.left(1));
    QCOMPARE(dispatcher.inputCommands.size(), 0);
    press(emoji.right(1));
    QCOMPARE(dispatcher.inputCommands.size(), 1);
    QCOMPARE(dispatcher.inputCommands.constLast(), emoji.toUtf8());

    press(emoji.left(1));
    press(QStringLiteral("a"));
    QCOMPARE(dispatcher.inputCommands.size(), 2);
    QCOMPARE(dispatcher.inputCommands.constLast(), QByteArrayLiteral("a"));
}

void TerminalKernelTest::terminalViewPastesClipboardThroughBoundedAuthority()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("paste"),
        QStringLiteral("paste-subscription"),
        21,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("paste-incarnation")));
    makeTerminalReady(registry, subscription);

    auto* clipboard = QGuiApplication::clipboard();
    clipboard->setText(QStringLiteral("denied"), QClipboard::Clipboard);
    QKeyEvent denied(
        QEvent::KeyPress,
        Qt::Key_V,
        Qt::ControlModifier | Qt::ShiftModifier,
        QStringLiteral("V"));
    QCoreApplication::sendEvent(&view, &denied);
    QCOMPARE(dispatcher.inputCommands.size(), 0);

    view.setTerminalInteraction(true, false);
    clipboard->setText(QStringLiteral("accepted"), QClipboard::Clipboard);
    QKeyEvent accepted(
        QEvent::KeyPress,
        Qt::Key_V,
        Qt::ControlModifier | Qt::ShiftModifier,
        QStringLiteral("V"));
    QCoreApplication::sendEvent(&view, &accepted);
    QCOMPARE(dispatcher.inputCommands, QVector<QByteArray> {QByteArrayLiteral("accepted")});
    QCOMPARE(dispatcher.inputSubscriptions.constFirst().sessionId, subscription.sessionId);
    QCOMPARE(
        dispatcher.inputSubscriptions.constFirst().subscriptionId,
        subscription.subscriptionId);
    QCOMPARE(
        dispatcher.inputSubscriptions.constFirst().generation,
        subscription.generation);
    QCOMPARE(
        dispatcher.inputIncarnations.constFirst(),
        QStringLiteral("paste-incarnation"));

    QSignalSpy errors(&view, &kodosi::TerminalView::operationError);
    clipboard->setText(
        QStringLiteral("first\nsecond"),
        QClipboard::Clipboard);
    QCoreApplication::sendEvent(&view, &accepted);

    clipboard->setText(
        QStringLiteral("echo ") + QChar(0x03),
        QClipboard::Clipboard);
    QCoreApplication::sendEvent(&view, &accepted);

    registry.receiveData({
        subscription,
        1,
        QByteArrayLiteral("\x1b[?2004h"),
    });
    QCoreApplication::processEvents();
    clipboard->setText(
        QStringLiteral("first\nsecond"),
        QClipboard::Clipboard);
    QCoreApplication::sendEvent(&view, &accepted);
    const QVector<QByteArray> expectedPasteCommands {
        QByteArrayLiteral("accepted"),
        QByteArrayLiteral("first\rsecond"),
        QByteArrayLiteral("echo  "),
        QByteArrayLiteral("\x1b[200~first\nsecond\x1b[201~"),
    };
    QCOMPARE(dispatcher.inputCommands, expectedPasteCommands);
    QCOMPARE(errors.count(), 0);

    clipboard->setText(
        QString(1024 * 1024 + 1, QLatin1Char('x')),
        QClipboard::Clipboard);
    QKeyEvent oversized(
        QEvent::KeyPress,
        Qt::Key_V,
        Qt::ControlModifier | Qt::ShiftModifier,
        QStringLiteral("V"));
    QCoreApplication::sendEvent(&view, &oversized);
    QCOMPARE(dispatcher.inputCommands.size(), 4);
    QCOMPARE(errors.count(), 1);
    QVERIFY(errors.constLast().constFirst().toString().contains(
        QStringLiteral("queue is full")));
}

void TerminalKernelTest::terminalViewTracksShiftInsertPasteReleaseExactly()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("paste-release"),
        QStringLiteral("paste-release-subscription"),
        26,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("paste-release-incarnation")));
    makeTerminalReady(registry, subscription);

    QGuiApplication::clipboard()->setText(
        QStringLiteral("shift-insert"),
        QClipboard::Clipboard);
    QKeyEvent pastePress(
        QEvent::KeyPress,
        Qt::Key_Insert,
        Qt::ShiftModifier,
        QString {});
    QCoreApplication::sendEvent(&view, &pastePress);
    QCOMPARE(
        dispatcher.inputCommands,
        QVector<QByteArray> {QByteArrayLiteral("shift-insert")});

    QFocusEvent focusOut(QEvent::FocusOut);
    QCoreApplication::sendEvent(&view, &focusOut);
    view.setTerminalInteraction(false, false);
    QKeyEvent unrelatedVRelease(
        QEvent::KeyRelease,
        Qt::Key_V,
        Qt::NoModifier,
        QStringLiteral("v"));
    unrelatedVRelease.setAccepted(false);
    QCoreApplication::sendEvent(&view, &unrelatedVRelease);
    QVERIFY(!unrelatedVRelease.isAccepted());
    QCOMPARE(dispatcher.inputCommands.size(), 1);
}

void TerminalKernelTest::terminalProgramsWriteTheClipboardAndTheFocusedViewOfTheirOwnTerminalAnswersReads()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(800, 500);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(800, 500));
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("osc-clipboard"),
        QStringLiteral("osc-clipboard-subscription"),
        25,
    };
    view.setTerminalInteraction(true, false);
    view.setAsksBeforeClipboardRead(false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("osc-clipboard-incarnation")));
    makeTerminalReady(registry, subscription);
    window.show();

    auto* clipboard = QGuiApplication::clipboard();
    clipboard->setText(QStringLiteral("before"), QClipboard::Clipboard);
    registry.receiveData({
        subscription,
        1,
        QByteArrayLiteral("\x1b]52;c;a29kb3NpLWNsaXBib2FyZA==\x07"),
    });
    QTRY_COMPARE(
        clipboard->text(QClipboard::Clipboard),
        QStringLiteral("kodosi-clipboard"));

    QSignalSpy requests(&view, &kodosi::TerminalView::clipboardReadRequested);
    registry.receiveData({subscription, 2, QByteArrayLiteral("\x1b]52;c;?\x07")});
    QCoreApplication::processEvents();
    QCOMPARE(requests.count(), 0);
    QCOMPARE(dispatcher.inputCommands.size(), 0);

    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(window.isActive() && view.hasActiveFocus());
    registry.receiveData({subscription, 3, QByteArrayLiteral("\x1b]52;c;?\x07")});
    QTRY_COMPARE(
        dispatcher.inputCommands,
        QVector<QByteArray> {QByteArrayLiteral("\x1b]52;c;a29kb3NpLWNsaXBib2FyZA==\x07")});
    QCOMPARE(requests.count(), 0);

    QSignalSpy errors(&view, &kodosi::TerminalView::terminalError);
    for (std::uint64_t sequence = 4; sequence < 104; ++sequence) {
        registry.receiveData({
            subscription,
            sequence,
            "\x1b]52;c;" + QByteArray::number(sequence).toBase64() + "\x07",
        });
    }
    QTRY_COMPARE(clipboard->text(QClipboard::Clipboard), QStringLiteral("103"));
    QCOMPARE(errors.count(), 0);
}

void TerminalKernelTest::terminalProgramsReadTheClipboardOnlyWhenAllowed()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(400, 200);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(400, 200));
    view.setTerminalInteraction(true, true);
    QVERIFY(view.asksBeforeClipboardRead());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("osc-clipboard-read"),
        QStringLiteral("osc-clipboard-read-subscription"),
        1,
    };
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    window.show();
    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(window.isActive() && view.hasActiveFocus());
    QGuiApplication::clipboard()->setText(QStringLiteral("host text"), QClipboard::Clipboard);
    QSignalSpy requests(&view, &kodosi::TerminalView::clipboardReadRequested);

    registry.receiveData({subscription, 1, QByteArrayLiteral("\x1b]52;c;?\x07")});
    QTRY_COMPARE(requests.count(), 1);
    QCOMPARE(dispatcher.inputCommands.size(), 0);
    view.answerClipboardRead(true);
    QTRY_COMPARE(dispatcher.inputCommands.size(), 1);
    QCOMPARE(
        dispatcher.inputCommands.last(),
        QByteArrayLiteral("\x1b]52;c;") + QByteArrayLiteral("host text").toBase64()
            + QByteArrayLiteral("\x07"));

    registry.receiveData({subscription, 2, QByteArrayLiteral("\x1b]52;c;?\x1b\\")});
    QTRY_COMPARE(requests.count(), 2);
    view.answerClipboardRead(false);
    QTRY_COMPARE(dispatcher.inputCommands.size(), 2);
    QCOMPARE(dispatcher.inputCommands.last(), QByteArrayLiteral("\x1b]52;c;\x1b\\"));

    view.answerClipboardRead(true);
    QTest::qWait(20);
    QCOMPARE(dispatcher.inputCommands.size(), 2);
    window.hide();
}

void TerminalKernelTest::terminalViewWheelUpdatesFrameWithoutPtyInput()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("wheel-view"),
        QStringLiteral("wheel-view-subscription"),
        22,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("wheel-view-incarnation")));

    QByteArray output;
    for (int line = 0; line < 300; ++line) {
        output += QByteArrayLiteral("line-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        6,
        20,
        checkpointFor(output, 20, 6),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();
    const auto before = view.accessibleText();
    QVERIFY(before.contains(QStringLiteral("line-299")));

    QSignalSpy frames(&view, &kodosi::TerminalView::frameChanged);
    QWheelEvent firstHalfStep(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, 60),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&view, &firstHalfStep);
    QCoreApplication::processEvents();
    QCOMPARE(view.accessibleText(), before);
    QCOMPARE(frames.count(), 0);

    QWheelEvent secondHalfStep(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, 60),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&view, &secondHalfStep);
    QTRY_VERIFY_WITH_TIMEOUT(view.accessibleText() != before, 250);
    const auto afterOneRow = view.accessibleText();
    QVERIFY(afterOneRow.contains(QStringLiteral("line-294")));
    QCOMPARE(frames.count(), 1);

    QWheelEvent boundedLargeDelta(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, 120 * 1000),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&view, &boundedLargeDelta);
    QTRY_VERIFY_WITH_TIMEOUT(view.accessibleText() != afterOneRow, 250);
    QVERIFY(!view.accessibleText().contains(QStringLiteral("line-00")));
    QVERIFY(frames.count() >= 1);
    QCOMPARE(dispatcher.inputCommands.size(), 0);
    QVERIFY(boundedLargeDelta.isAccepted());
}

void TerminalKernelTest::terminalViewAcceptedInputReturnsViewportToBottom()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("input-bottom"),
        QStringLiteral("input-bottom-subscription"),
        27,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("input-bottom-incarnation")));

    QByteArray output;
    for (int line = 0; line < 40; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        5,
        20,
        checkpointFor(output, 20, 5),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    const auto scrollToTop = [&] {
        QWheelEvent wheel(
            QPointF(20, 20),
            QPointF(20, 20),
            {},
            QPoint(0, 120 * 100),
            Qt::NoButton,
            Qt::NoModifier,
            Qt::ScrollUpdate,
            false);
        QCoreApplication::sendEvent(&view, &wheel);
        QTRY_VERIFY_WITH_TIMEOUT(
            view.accessibleText().contains(QStringLiteral("row-00")),
            250);
    };
    const auto verifyAtBottom = [&] {
        QTRY_VERIFY_WITH_TIMEOUT(
            view.accessibleText().contains(QStringLiteral("row-39")),
            250);
        QVERIFY(!view.accessibleText().contains(QStringLiteral("row-00")));
    };

    scrollToTop();
    QKeyEvent key(
        QEvent::KeyPress,
        Qt::Key_A,
        Qt::NoModifier,
        QStringLiteral("a"));
    QCoreApplication::sendEvent(&view, &key);
    verifyAtBottom();

    scrollToTop();
    QInputMethodEvent ime;
    ime.setCommitString(QStringLiteral("ime"));
    QCoreApplication::sendEvent(&view, &ime);
    verifyAtBottom();

    scrollToTop();
    QGuiApplication::clipboard()->setText(
        QStringLiteral("paste"),
        QClipboard::Clipboard);
    QKeyEvent paste(
        QEvent::KeyPress,
        Qt::Key_Insert,
        Qt::ShiftModifier,
        QString {});
    QCoreApplication::sendEvent(&view, &paste);
    verifyAtBottom();

    scrollToTop();
    view.setTerminalInteraction(false, false);
    const auto inputCount = dispatcher.inputCommands.size();
    QKeyEvent denied(
        QEvent::KeyPress,
        Qt::Key_B,
        Qt::NoModifier,
        QStringLiteral("b"));
    QCoreApplication::sendEvent(&view, &denied);
    QCoreApplication::processEvents();
    QVERIFY(view.accessibleText().contains(QStringLiteral("row-00")));
    QCOMPARE(dispatcher.inputCommands.size(), inputCount);
}

void TerminalKernelTest::terminalViewSelectionAnchorSurvivesPresentedScroll()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("selection-anchor"),
        QStringLiteral("selection-anchor-subscription"),
        28,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("selection-anchor-incarnation")));

    QByteArray output;
    for (int line = 0; line < 20; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        5,
        12,
        checkpointFor(output, 12, 5),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    QWheelEvent top(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, 120 * 100),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&view, &top);
    QTRY_VERIFY_WITH_TIMEOUT(
        view.accessibleText().contains(QStringLiteral("row-00")),
        250);

    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF anchor(cell.width() * 0.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        anchor,
        anchor,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);

    QWheelEvent downOne(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, -120),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&view, &downOne);
    QTRY_VERIFY_WITH_TIMEOUT(
        view.accessibleText().startsWith(QStringLiteral("row-01")),
        250);

    const QPointF endpoint(cell.width() * 5.5, cell.height() * 0.5);
    QMouseEvent move(
        QEvent::MouseMove,
        endpoint,
        endpoint,
        Qt::NoButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        endpoint,
        endpoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &move);
    QCoreApplication::sendEvent(&view, &release);

    const auto text = registry.selectedText(kodosi::TerminalSurfaceIdentity {subscription, 1});
    QVERIFY(text);
    QVERIFY(text->contains(QStringLiteral("row-00")));
    QVERIFY(text->contains(QStringLiteral("\nrow-")));
}

void TerminalKernelTest::terminalViewCancelsSelectionAgainstStaleDisplayedViewport()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("selection-stale"),
        QStringLiteral("selection-stale-subscription"),
        29,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("selection-stale-incarnation")));

    QByteArray output;
    for (int line = 0; line < 20; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        5,
        12,
        checkpointFor(output, 12, 5),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();
    QVERIFY(registry.scrollViewport(kodosi::TerminalSurfaceIdentity {subscription, 1}, -100));
    QCoreApplication::processEvents();

    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF anchor(cell.width() * 0.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        anchor,
        anchor,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);

    QSignalSpy errors(&view, &kodosi::TerminalView::terminalError);
    const auto advanced = registry.scrollViewport(kodosi::TerminalSurfaceIdentity {subscription, 1}, 1);
    QVERIFY(advanced);
    const QPointF endpoint(cell.width() * 5.5, cell.height() * 0.5);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        endpoint,
        endpoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &release);
    QCoreApplication::processEvents();

    const auto text = registry.selectedText(kodosi::TerminalSurfaceIdentity {subscription, 1});
    QVERIFY(text);
    QVERIFY(text->isEmpty());

    const auto outside = registry.beginSelection(
        kodosi::TerminalSurfaceIdentity {subscription, 1},
        (*advanced)->viewportRevision,
        (*advanced)->columns,
        0);
    QVERIFY(!outside);
    QCOMPARE(
        outside.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::HitTestRace);

    const auto missingAnchor = registry.updateSelection(
        kodosi::TerminalSurfaceIdentity {subscription, 1},
        (*advanced)->viewportRevision,
        0,
        0);
    QVERIFY(!missingAnchor);
    QCOMPARE(
        missingAnchor.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::HitTestRace);
    QCoreApplication::processEvents();
    QCOMPARE(errors.count(), 0);
}

void TerminalKernelTest::terminalLinkValidationRejectsMalformedValues()
{
    QVERIFY(kodosi::validatedTerminalLink(
        QStringLiteral("https://example.com/path")));
    QVERIFY(kodosi::validatedTerminalLink(
        QStringLiteral("mailto:user@example.com")));
    QVERIFY(kodosi::validatedTerminalLink(
        QStringLiteral("file:///home/user/project/main.rs")));
    QVERIFY(kodosi::validatedTerminalLink(
        QStringLiteral("vscode://file/home/user/project/main.rs:12")));
    QVERIFY(!kodosi::validatedTerminalLink(
        QStringLiteral("project/main.rs")));
    QVERIFY(!kodosi::validatedTerminalLink(
        QStringLiteral("https://example.com/\nnext")));
    QVERIFY(!kodosi::validatedTerminalLink(
        QStringLiteral("https://example.com/%0a")));
    QVERIFY(!kodosi::validatedTerminalLink(
        QStringLiteral("https://example.com/\u202Etxt")));
    QVERIFY(!kodosi::validatedTerminalLink(
        QStringLiteral("https://example.com/%E2%80%AEtxt")));
}

void TerminalKernelTest::terminalViewActivatesGhosttyLinks()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("link"),
        QStringLiteral("link-subscription"),
        23,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("link-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(
            QByteArrayLiteral(
                "\x1b]8;;https://example.com/safe\x1b\\safe\x1b]8;;\x1b\\ "
                "\x1b]8;;file:///home/user/secret\x1b\\file\x1b]8;;\x1b\\"),
            40,
            4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    UrlCapture capture;
    QDesktopServices::setUrlHandler(
        QStringLiteral("https"),
        &capture,
        "capture");
    QDesktopServices::setUrlHandler(
        QStringLiteral("file"),
        &capture,
        "capture");
    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF safePoint(cell.width() * 0.5, cell.height() * 0.5);
    QHoverEvent hover(
        QEvent::HoverMove,
        safePoint,
        safePoint,
        safePoint,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &hover);
    QCOMPARE(view.cursor().shape(), Qt::PointingHandCursor);

    QMouseEvent press(
        QEvent::MouseButtonPress,
        safePoint,
        safePoint,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        safePoint,
        safePoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);
    QTRY_COMPARE_WITH_TIMEOUT(capture.values.size(), 1, 250);
    QCOMPARE(
        capture.values.constFirst(),
        QUrl(QStringLiteral("https://example.com/safe")));

    view.setTerminalInteraction(false, false);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);
    QTest::qWait(20);
    QCOMPARE(capture.values.size(), 1);
    view.setTerminalInteraction(true, false);

    const QPointF filePoint(cell.width() * 6.5, cell.height() * 0.5);
    QHoverEvent fileHover(
        QEvent::HoverMove,
        filePoint,
        filePoint,
        safePoint,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &fileHover);
    QCOMPARE(view.cursor().shape(), Qt::PointingHandCursor);
    QMouseEvent filePress(
        QEvent::MouseButtonPress,
        filePoint,
        filePoint,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent fileRelease(
        QEvent::MouseButtonRelease,
        filePoint,
        filePoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &filePress);
    QCoreApplication::sendEvent(&view, &fileRelease);
    QTRY_COMPARE_WITH_TIMEOUT(capture.values.size(), 2, 250);
    QCOMPARE(
        capture.values.constLast(),
        QUrl(QStringLiteral("file:///home/user/secret")));
    QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
    QDesktopServices::unsetUrlHandler(QStringLiteral("file"));
}

void TerminalKernelTest::terminalViewOpensAWebAddressInTheTextWithControlClick()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("text-link"),
        QStringLiteral("text-link-subscription"),
        23,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("text-link-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(QByteArrayLiteral("go https://example.com/page now"), 40, 4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    UrlCapture capture;
    QDesktopServices::setUrlHandler(QStringLiteral("https"), &capture, "capture");
    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF point(cell.width() * 8.5, cell.height() * 0.5);
    const auto click = [&](const Qt::KeyboardModifiers modifiers) {
        QMouseEvent press(QEvent::MouseButtonPress, point, point, Qt::LeftButton, Qt::LeftButton, modifiers);
        QMouseEvent release(QEvent::MouseButtonRelease, point, point, Qt::LeftButton, Qt::NoButton, modifiers);
        QCoreApplication::sendEvent(&view, &press);
        QCoreApplication::sendEvent(&view, &release);
    };
    click(Qt::NoModifier);
    QTest::qWait(20);
    QVERIFY(capture.values.isEmpty());
    QCOMPARE(*registry.selectedText(kodosi::TerminalSurfaceIdentity {subscription, 1}), QString());

    QHoverEvent hover(QEvent::HoverMove, point, point, point, Qt::ControlModifier);
    QCoreApplication::sendEvent(&view, &hover);
    QCOMPARE(view.cursor().shape(), Qt::PointingHandCursor);
    click(Qt::ControlModifier);
    QTRY_COMPARE_WITH_TIMEOUT(capture.values.size(), 1, 250);
    QCOMPARE(capture.values.constFirst(), QUrl(QStringLiteral("https://example.com/page")));
    QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
}

void TerminalKernelTest::terminalViewCannotActivateLinkFromUndisplayedFrame()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("link-stale"),
        QStringLiteral("link-stale-subscription"),
        30,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("link-stale-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(QByteArrayLiteral("plain"), 40, 4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();
    QVERIFY(view.accessibleText().startsWith(QStringLiteral("plain")));

    registry.receiveData({
        subscription,
        1,
        QByteArrayLiteral(
            "\r\x1b[2K\x1b]8;;https://example.com/not-visible\x1b\\link"
            "\x1b]8;;\x1b\\"),
    });

    UrlCapture capture;
    QDesktopServices::setUrlHandler(
        QStringLiteral("https"),
        &capture,
        "capture");
    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF point(cell.width() * 0.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        point,
        point,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        point,
        point,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);
    QCoreApplication::processEvents();
    QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
    QCOMPARE(capture.values.size(), 0);
}

void TerminalKernelTest::terminalViewCancelsLinkReleaseOutsideBounds()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("link-bounds"),
        QStringLiteral("link-bounds-subscription"),
        31,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("link-bounds-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(
            QByteArrayLiteral(
                "\x1b]8;;https://example.com/edge\x1b\\x"
                "\x1b]8;;\x1b\\"),
            40,
            4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();

    UrlCapture capture;
    QDesktopServices::setUrlHandler(
        QStringLiteral("https"),
        &capture,
        "capture");
    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        view.lineHeight());
    const QPointF edge(cell.width() * 0.5, cell.height() * 0.5);
    const QPointF outside(-20.0, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        edge,
        edge,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        outside,
        outside,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);
    QCoreApplication::processEvents();
    QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
    QCOMPARE(capture.values.size(), 0);
}

void TerminalKernelTest::terminalViewCopySelectionStillUsesHostClipboard()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("copy"),
        QStringLiteral("copy-subscription"),
        24,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(false, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("copy-incarnation")));
    makeTerminalReady(registry, subscription);
    const QSizeF scaledCell(view.gridSize().width() / 80, view.gridSize().height() / 24);
    const QPointF anchor(scaledCell.width() * view.viewportScale() * 0.5,
        scaledCell.height() * view.viewportScale() * 0.5);
    const QPointF endpoint(scaledCell.width() * view.viewportScale() * 4.5,
        scaledCell.height() * view.viewportScale() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        anchor,
        anchor,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent move(
        QEvent::MouseMove,
        endpoint,
        endpoint,
        Qt::NoButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        endpoint,
        endpoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &move);
    QCoreApplication::sendEvent(&view, &release);

    QGuiApplication::clipboard()->clear(QClipboard::Clipboard);
    QKeyEvent copy(
        QEvent::KeyPress,
        Qt::Key_C,
        Qt::ControlModifier | Qt::ShiftModifier,
        QStringLiteral("C"));
    QCoreApplication::sendEvent(&view, &copy);
    QCOMPARE(
        QGuiApplication::clipboard()->text(QClipboard::Clipboard),
        QStringLiteral("ready"));
    QCOMPARE(dispatcher.inputCommands.size(), 0);
}

void TerminalKernelTest::terminalViewRetriesRevocationBlurBeforeRefocus()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        8,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);

    QFocusEvent focus(QEvent::FocusIn);
    QCoreApplication::sendEvent(&view, &focus);
    QCOMPARE(dispatcher.terminalCommands.size(), 1);
    dispatcher.busyTerminalCommands = 1;
    view.setTerminalInteraction(false, false);
    view.setTerminalInteraction(true, false);
    QCOMPARE(dispatcher.terminalCommands.size(), 2);

    QTRY_COMPARE_WITH_TIMEOUT(
        dispatcher.terminalCommands.size(),
        4,
        250);
    const QStringList types {
        dispatcher.terminalCommands.at(0)
            .value(QStringLiteral("type")).toString(),
        dispatcher.terminalCommands.at(1)
            .value(QStringLiteral("type")).toString(),
        dispatcher.terminalCommands.at(2)
            .value(QStringLiteral("type")).toString(),
        dispatcher.terminalCommands.at(3)
            .value(QStringLiteral("type")).toString(),
    };
    QCOMPARE(
        types,
        QStringList({
            QStringLiteral("session.focus"),
            QStringLiteral("session.blur"),
            QStringLiteral("session.blur"),
            QStringLiteral("session.focus"),
        }));
}

void TerminalKernelTest::terminalViewWaitsQuietlyForItsConnection()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    dispatcher.unconnectedTerminalCommands = 1;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("waiting"),
        QStringLiteral("waiting-subscription"),
        12,
    };
    QSignalSpy errors(&view, &kodosi::TerminalView::operationError);
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(false, true);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        24,
        80,
        checkpointFor(QByteArrayLiteral("ready"), 80, 24),
    }));
    QTRY_COMPARE_WITH_TIMEOUT(dispatcher.terminalCommands.size(), 1, 250);
    QTest::qWait(60);
    QCOMPARE(dispatcher.terminalCommands.size(), 1);
    QVERIFY(errors.isEmpty());

    registry.receiveConnectResult({subscription, ffiOk});
    QTRY_COMPARE_WITH_TIMEOUT(dispatcher.terminalCommands.size(), 2, 250);
    QCOMPARE(
        dispatcher.terminalCommands.constLast()
            .value(QStringLiteral("type"))
            .toString(),
        QStringLiteral("session.resize"));
    QVERIFY(errors.isEmpty());
}

void TerminalKernelTest::terminalViewSendsItsSizeOnlyWhileItMayResize()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    dispatcher.busyTerminalCommands = 1;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        9,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(false, true);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    QTRY_VERIFY_WITH_TIMEOUT(
        dispatcher.terminalCommands.size() >= 2,
        250);
    for (const auto& command : std::as_const(dispatcher.terminalCommands)) {
        QCOMPARE(
            command.value(QStringLiteral("type")).toString(),
            QStringLiteral("session.resize"));
        QVERIFY(!command.contains(QStringLiteral("claim")));
        QCOMPARE(
            command.value(QStringLiteral("sessionId")).toString(),
            QStringLiteral("session"));
        QCOMPARE(
            command.value(QStringLiteral("subscriptionId")).toString(),
            QStringLiteral("subscription"));
        QCOMPARE(
            command.value(QStringLiteral("subscriptionGeneration"))
                .toInteger(),
            9);
    }

    kodosi::TerminalView denied(m_window.contentItem());
    denied.setWidth(800);
    denied.setHeight(400);
    denied.setTerminalInteraction(false, false);
    const kodosi::TerminalSubscription deniedSubscription {
        QStringLiteral("denied"),
        QStringLiteral("denied-subscription"),
        10,
    };
    const auto beforeDenied = dispatcher.terminalCommands.size();
    QVERIFY(denied.attach(
        registry,
        dispatcher,
        deniedSubscription,
        QStringLiteral("denied-incarnation")));
    makeTerminalReady(registry, deniedSubscription);
    QTest::qWait(20);
    QCOMPARE(dispatcher.terminalCommands.size(), beforeDenied);
    denied.setTerminalInteraction(false, true);
    QTRY_COMPARE_WITH_TIMEOUT(
        dispatcher.terminalCommands.size(),
        beforeDenied + 1,
        250);
}

void TerminalKernelTest::terminalViewDefersResizeWhileEffectivelyHidden()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.setGeometry(0, 0, 900, 500);
    window.show();
    QQuickItem container(window.contentItem());
    kodosi::TerminalView view(&container);
    QCoreApplication::processEvents();
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("hidden"),
        QStringLiteral("hidden-subscription"),
        12,
    };
    view.setWidth(800);
    view.setHeight(400);
    view.setTerminalInteraction(false, true);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("hidden-incarnation")));
    makeTerminalReady(registry, subscription);
    QTRY_COMPARE_WITH_TIMEOUT(
        dispatcher.terminalCommands.size(),
        1,
        250);

    container.setVisible(false);
    QVERIFY(!view.isVisible());
    view.setWidth(640);
    QTest::qWait(20);
    QCOMPARE(dispatcher.terminalCommands.size(), 1);

    container.setVisible(true);
    QVERIFY(view.isVisible());
    QTRY_COMPARE_WITH_TIMEOUT(
        dispatcher.terminalCommands.size(),
        2,
        250);
}

void TerminalKernelTest::surfaceControllerFencesSessionIncarnations()
{
    kodosi::TerminalSessionRegistry registry;
    kodosi::RuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(registry, runtime, sessions);
    kodosi::TerminalView view(m_window.contentItem());
    QSignalSpy rejected(
        &controller,
        &kodosi::TerminalSurfaceController::attachmentRejected);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000001","incarnationId":"0198aaaa-0000-7000-8000-000000000065","name":"One","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(
        &view,
        QStringLiteral("0198aaaa-0000-7000-8000-000000000001")));
    QVERIFY(controller.retry(
        &view,
        QStringLiteral("0198aaaa-0000-7000-8000-000000000001")));
    QVERIFY(!controller.bind(
        &view,
        QString {}));
    QCOMPARE(rejected.count(), 1);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[]})")).object());
    QCOMPARE(rejected.count(), 2);
    QVERIFY(!view.terminalReady());
}

void TerminalKernelTest::surfaceControllerKeepsBindingsIndependent()
{
    kodosi::TerminalSessionRegistry registry;
    FakeRuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(registry, runtime, sessions);
    kodosi::TerminalView firstView(m_window.contentItem());
    kodosi::TerminalView secondView(m_window.contentItem());
    QSignalSpy rejected(
        &controller,
        &kodosi::TerminalSurfaceController::attachmentRejected);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000002","incarnationId":"0198aaaa-0000-7000-8000-000000000065","name":"First","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]},{"id":"0198aaaa-0000-7000-8000-000000000003","incarnationId":"0198aaaa-0000-7000-8000-000000000066","name":"Second","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(&firstView, QStringLiteral("0198aaaa-0000-7000-8000-000000000002")));
    QVERIFY(controller.bind(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000003")));
    QCOMPARE(runtime.connectCount, 2);
    QVERIFY(firstView.canSendInput());
    QVERIFY(secondView.canSendInput());

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000002","incarnationId":"0198aaaa-0000-7000-8000-000000000065","name":"First","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]},{"id":"0198aaaa-0000-7000-8000-000000000003","incarnationId":"0198aaaa-0000-7000-8000-000000000066","name":"Second","kind":"remote","status":"running","connectionState":"connected","isOwner":false,"sharedWith":[]}]})")).object());
    QVERIFY(firstView.canSendInput());
    QVERIFY(secondView.canSendInput());
    QCOMPARE(runtime.connectCount, 2);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000002","incarnationId":"0198aaaa-0000-7000-8000-000000000065","name":"First","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]},{"id":"0198aaaa-0000-7000-8000-000000000003","incarnationId":"0198aaaa-0000-7000-8000-000000000066","name":"Second","kind":"remote","status":"running","connectionState":"connected","isOwner":false,"sharedWith":[]}]})")).object());
    QVERIFY(firstView.canSendInput());
    QVERIFY(secondView.canSendInput());
    QVERIFY(secondView.canResize());
    QCOMPARE(runtime.connectCount, 2);
    controller.detach(&firstView);
    QCOMPARE(runtime.disconnectCount, 1);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000003","incarnationId":"0198aaaa-0000-7000-8000-000000000066","name":"Second","kind":"remote","status":"running","connectionState":"connected","isOwner":false,"sharedWith":[]}]})")).object());
    QCOMPARE(rejected.count(), 0);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000003","incarnationId":"0198aaaa-0000-7000-8000-000000000067","name":"Second","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());
    QCOMPARE(rejected.count(), 0);
    QCOMPARE(runtime.connectCount, 3);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[]})")).object());
    QCOMPARE(rejected.count(), 1);
    QCOMPARE(
        rejected.constFirst().at(1).toString(),
        QStringLiteral("0198aaaa-0000-7000-8000-000000000003"));
    QVERIFY(!firstView.terminalReady());
    QVERIFY(!secondView.terminalReady());
}

void TerminalKernelTest::surfaceControllerKeepsSameSessionSurfacesIndependent()
{
    kodosi::TerminalSessionRegistry registry;
    FakeRuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(registry, runtime, sessions);
    kodosi::TerminalView stageView(m_window.contentItem());
    kodosi::TerminalView focusView(m_window.contentItem());
    stageView.setWidth(400);
    stageView.setHeight(160);
    focusView.setWidth(400);
    focusView.setHeight(160);
    QSignalSpy stageConnections(
        &stageView,
        &kodosi::TerminalView::connectionCompleted);
    QSignalSpy focusConnections(
        &focusView,
        &kodosi::TerminalView::connectionCompleted);
    QSignalSpy stageFrames(
        &stageView,
        &kodosi::TerminalView::frameChanged);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000004","incarnationId":"0198aaaa-0000-7000-8000-000000000068","name":"Shared","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(&stageView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.connectedSubscriptions.size(), 1);
    const auto stageSubscription = runtime.connectedSubscriptions.constFirst();

    QByteArray output;
    for (int line = 0; line < 20; ++line) {
        output += QByteArrayLiteral("row-");
        output += QByteArray::number(line).rightJustified(2, '0');
        output += QByteArrayLiteral("\r\n");
    }
    QVERIFY(registry.installCheckpoint({
        stageSubscription,
        1,
        5,
        12,
        checkpointFor(output, 12, 5),
    }));
    registry.receiveConnectResult({stageSubscription, ffiOk});
    QCoreApplication::processEvents();

    QVERIFY(stageView.terminalReady());
    QCOMPARE(stageConnections.size(), 1);

    QWheelEvent top(
        QPointF(20, 20),
        QPointF(20, 20),
        {},
        QPoint(0, 120 * 100),
        Qt::NoButton,
        Qt::NoModifier,
        Qt::ScrollUpdate,
        false);
    QCoreApplication::sendEvent(&stageView, &top);
    QTRY_VERIFY_WITH_TIMEOUT(
        stageView.accessibleText().contains(QStringLiteral("row-00")),
        250);
    const auto cell = kodosi::TerminalRasterizer::cellSize(
        QFontDatabase::systemFont(QFontDatabase::FixedFont),
        stageView.lineHeight());
    const QPointF anchor(cell.width() * 0.5, cell.height() * 0.5);
    const QPointF endpoint(cell.width() * 5.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        anchor,
        anchor,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent move(
        QEvent::MouseMove,
        endpoint,
        endpoint,
        Qt::NoButton,
        Qt::LeftButton,
        Qt::NoModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        endpoint,
        endpoint,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::NoModifier);
    QCoreApplication::sendEvent(&stageView, &press);
    QCoreApplication::sendEvent(&stageView, &move);
    QCoreApplication::sendEvent(&stageView, &release);
    QTRY_VERIFY_WITH_TIMEOUT(
        stageView.accessibleSelection().first
            != stageView.accessibleSelection().second,
        250);
    const auto stageText = stageView.accessibleText();
    const auto stageSelection = stageView.accessibleSelection();

    bool refreshCheckpointInstalled = false;
    runtime.refreshHandler =
        [&](const kodosi::TerminalSubscription& subscription) {
            refreshCheckpointInstalled =
                registry.installCheckpoint({
                    subscription,
                    1,
                    5,
                    12,
                    checkpointFor(output, 12, 5),
                });
        };
    QVERIFY(controller.bind(&focusView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.refreshedSubscriptions.size(), 1);
    const auto focusSubscription = runtime.refreshedSubscriptions.constFirst();
    QCOMPARE(stageSubscription.sessionId, focusSubscription.sessionId);
    QCOMPARE(stageSubscription.subscriptionId, focusSubscription.subscriptionId);
    QCOMPARE(stageSubscription.generation, focusSubscription.generation);
    QVERIFY(refreshCheckpointInstalled);
    QCoreApplication::processEvents();

    QVERIFY(stageView.terminalReady());
    QVERIFY(focusView.terminalReady());
    QCOMPARE(stageView.accessibleText(), stageText);
    QCOMPARE(stageView.accessibleSelection(), stageSelection);
    QVERIFY(focusView.accessibleText().contains(QStringLiteral("row-19")));

    controller.detach(&focusView);
    runtime.refreshHandler = {};
    QVERIFY(controller.bind(&focusView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    controller.detach(&focusView);
    QVERIFY(registry.installCheckpoint({ stageSubscription, 1, 5, 12, checkpointFor(output, 12, 5) }));
    QCOMPARE(runtime.disconnectedSubscriptions.size(), 0);
    QVERIFY(!focusView.terminalReady());
    QVERIFY(stageView.terminalReady());
    QCOMPARE(stageView.accessibleText(), stageText);
    QCOMPARE(stageView.accessibleSelection(), stageSelection);

    const auto stageFrameCount = stageFrames.count();
    registry.receiveData({
        stageSubscription,
        1,
        QByteArrayLiteral(" live"),
    });
    QCoreApplication::processEvents();
    QVERIFY(stageView.terminalReady());
    QVERIFY(stageFrames.count() > stageFrameCount);

    controller.detach(&stageView);
    QCOMPARE(runtime.disconnectedSubscriptions.size(), 1);
    QCOMPARE(
        runtime.disconnectedSubscriptions.constFirst().subscriptionId,
        stageSubscription.subscriptionId);
}

void TerminalKernelTest::surfaceControllerRetriesFailedSeedRefresh()
{
    kodosi::TerminalSessionRegistry registry;
    FakeRuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(registry, runtime, sessions);
    kodosi::TerminalView firstView(m_window.contentItem());
    kodosi::TerminalView secondView(m_window.contentItem());
    firstView.setWidth(400);
    firstView.setHeight(160);
    secondView.setWidth(400);
    secondView.setHeight(160);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000004","incarnationId":"0198aaaa-0000-7000-8000-000000000068","name":"Shared","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(&firstView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.connectedSubscriptions.size(), 1);
    const auto subscription = runtime.connectedSubscriptions.constFirst();
    makeTerminalReady(registry, subscription);
    QVERIFY(firstView.terminalReady());

    runtime.refreshFailures = 1;
    QVERIFY(controller.bind(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.refreshCount, 1);
    QVERIFY(firstView.terminalReady());
    QVERIFY(!secondView.terminalReady());

    bool refreshCheckpointInstalled = false;
    runtime.refreshHandler =
        [&](const kodosi::TerminalSubscription& refreshed) {
            refreshCheckpointInstalled =
                registry.installCheckpoint({
                    refreshed,
                    1,
                    24,
                    80,
                    checkpointFor(QByteArrayLiteral("retry"), 80, 24),
                });
        };
    QVERIFY(controller.retry(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.refreshCount, 2);
    QVERIFY(refreshCheckpointInstalled);
    QCoreApplication::processEvents();

    QVERIFY(firstView.terminalReady());
    QVERIFY(secondView.terminalReady());
    QVERIFY(secondView.accessibleText().contains(QStringLiteral("retry")));
}

void TerminalKernelTest::surfaceControllerTimesOutAcceptedSeedRefresh()
{
    kodosi::TerminalSessionRegistry registry;
    FakeRuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(
        registry,
        runtime,
        sessions,
        50);
    kodosi::TerminalView firstView(m_window.contentItem());
    kodosi::TerminalView secondView(m_window.contentItem());
    firstView.setWidth(400);
    firstView.setHeight(160);
    secondView.setWidth(400);
    secondView.setHeight(160);
    QSignalSpy rejected(
        &controller,
        &kodosi::TerminalSurfaceController::attachmentRejected);

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000004","incarnationId":"0198aaaa-0000-7000-8000-000000000068","name":"Shared","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(&firstView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    const auto subscription = runtime.connectedSubscriptions.constFirst();
    makeTerminalReady(registry, subscription);
    QVERIFY(firstView.terminalReady());

    QVERIFY(controller.bind(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.refreshCount, 1);
    QVERIFY(firstView.terminalReady());
    QVERIFY(!secondView.terminalReady());
    QTRY_COMPARE_WITH_TIMEOUT(rejected.count(), 1, 250);
    QCOMPARE(
        rejected.constFirst().at(0).value<kodosi::TerminalView*>(),
        &secondView);
    QVERIFY(firstView.terminalReady());
    QVERIFY(!secondView.terminalReady());

    bool refreshCheckpointInstalled = false;
    runtime.refreshHandler =
        [&](const kodosi::TerminalSubscription& refreshed) {
            refreshCheckpointInstalled =
                registry.installCheckpoint({
                    refreshed,
                    3,
                    24,
                    80,
                    checkpointFor(QByteArrayLiteral("retry"), 80, 24),
                });
        };
    QVERIFY(controller.retry(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(runtime.refreshCount, 2);
    QVERIFY(refreshCheckpointInstalled);
    QCoreApplication::processEvents();

    QVERIFY(firstView.terminalReady());
    QVERIFY(secondView.terminalReady());
    QVERIFY(secondView.accessibleText().contains(QStringLiteral("retry")));
}

void TerminalKernelTest::surfaceControllerScopesAttachmentOutcomes()
{
    kodosi::TerminalSessionRegistry registry;
    FakeRuntimeBridge runtime(registry);
    kodosi::SessionCatalogModel sessions;
    kodosi::TerminalSurfaceController controller(registry, runtime, sessions);
    kodosi::TerminalView firstView(m_window.contentItem());
    kodosi::TerminalView secondView(m_window.contentItem());
    QString firstState = QStringLiteral("healthy");
    QString secondState = QStringLiteral("connecting");

    connect(
        &controller,
        &kodosi::TerminalSurfaceController::attachmentRejected,
        this,
        [&](kodosi::TerminalView* surface, const QString&, const QString& reason) {
            if (surface == &firstView) {
                firstState = reason;
            } else if (surface == &secondView) {
                secondState = reason;
            }
        });
    connect(
        &controller,
        &kodosi::TerminalSurfaceController::attachmentReady,
        this,
        [&](kodosi::TerminalView* surface, const QString&) {
            if (surface == &firstView) {
                firstState = QStringLiteral("ready");
            } else if (surface == &secondView) {
                secondState = QStringLiteral("ready");
            }
        });

    sessions.apply(QJsonDocument::fromJson(QByteArrayLiteral(R"({"type":"sessions.snapshot","sessions":[{"id":"0198aaaa-0000-7000-8000-000000000004","incarnationId":"0198aaaa-0000-7000-8000-000000000068","name":"Shared","kind":"local","status":"running","connectionState":"local","isOwner":true,"sharedWith":[]}]})")).object());

    QVERIFY(controller.bind(&firstView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    const auto subscription = runtime.connectedSubscriptions.constFirst();
    makeTerminalReady(registry, subscription);
    QCOMPARE(firstState, QStringLiteral("ready"));

    firstState = QStringLiteral("healthy");
    runtime.refreshFailures = 1;
    QVERIFY(controller.bind(&secondView, QStringLiteral("0198aaaa-0000-7000-8000-000000000004")));
    QCOMPARE(firstState, QStringLiteral("healthy"));
    QCOMPARE(
        secondState,
        QStringLiteral("The terminal could not connect."));
    QVERIFY(firstView.terminalReady());
    QVERIFY(!secondView.terminalReady());
}

void TerminalKernelTest::registryRetiresAbandonedSeedCapacity()
{
    kodosi::TerminalSessionRegistry registry;
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("seed-capacity"),
        QStringLiteral("seed-capacity-subscription"),
        32,
    };
    const kodosi::TerminalSurfaceIdentity primary {
        .subscription = subscription,
        .surfaceGeneration = 1,
    };
    const auto primaryRegistration =
        registry.registerSurface(primary, {});
    QVERIFY(primaryRegistration.has_value());
    QVERIFY(primaryRegistration->requiresConnection);
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        24,
        80,
        checkpointFor(QByteArrayLiteral("ready"), 80, 24),
    }));

    for (std::uint64_t generation = 2; generation < 72; ++generation) {
        const kodosi::TerminalSurfaceIdentity secondary {
            .subscription = subscription,
            .surfaceGeneration = generation,
        };
        const auto registration =
            registry.registerSurface(secondary, {});
        QVERIFY(registration.has_value());
        QVERIFY(registration->requiresRefresh);
        QVERIFY(!registry.unregisterSurface(secondary));
    }
}

void TerminalKernelTest::registryRoutesMultiSurfaceControlExactly()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("shared-registry"),
        QStringLiteral("shared-subscription"),
        31,
    };
    const kodosi::TerminalSurfaceIdentity first {
        .subscription = subscription,
        .surfaceGeneration = 41,
    };
    const kodosi::TerminalSurfaceIdentity second {
        .subscription = subscription,
        .surfaceGeneration = 42,
    };
    int firstFrames = 0;
    int secondFrames = 0;
    int firstResizeOutcomes = 0;
    int secondResizeOutcomes = 0;
    kodosi::TerminalSessionRegistry registry;
    const auto firstRegistration = registry.registerSurface(
        first,
        {
            .frameChanged = [&](auto) { ++firstFrames; },
            .resizeCompleted = [&](auto) { ++firstResizeOutcomes; },
        });
    QVERIFY(firstRegistration);
    QVERIFY(firstRegistration->requiresConnection);
    const auto secondRegistration = registry.registerSurface(
        second,
        {
            .frameChanged = [&](auto) { ++secondFrames; },
            .resizeCompleted = [&](auto) { ++secondResizeOutcomes; },
        });
    QVERIFY(secondRegistration);
    QVERIFY(!secondRegistration->requiresConnection);
    QVERIFY(!secondRegistration->requiresRefresh);
    QVERIFY(!registry.registerSurface(
        {
            .subscription = {
                QStringLiteral("shared-registry"),
                QStringLiteral("replacement-subscription"),
                32,
            },
            .surfaceGeneration = 43,
        },
        {}));

    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        3,
        24,
        checkpointFor(QByteArrayLiteral("shared"), 24, 3),
    }));
    QCOMPARE(firstFrames, 1);
    QCOMPARE(secondFrames, 1);

    registry.receiveControl({
        subscription,
        QByteArrayLiteral(
            R"({"type":"term.resizeApplied","sessionId":"shared-registry","requestId":"resize-second","expectedRuntimeIncarnationId":"incarnation","subscriptionId":"shared-subscription","subscriptionGeneration":31,"surfaceGeneration":42,"cols":24,"rows":3,"widthPixels":216,"heightPixels":54,"cellWidthPixels":9,"cellHeightPixels":18})"),
    });
    QCOMPARE(firstResizeOutcomes, 0);
    QCOMPARE(secondResizeOutcomes, 1);

    QVERIFY(!registry.registerSurface({subscription, 0}, {}));
    QVERIFY(!registry.encodeKey({subscription, 0}, {
        .key = kodosi::TerminalKey::Enter,
        .action = kodosi::TerminalKeyAction::Press,
    }));
    QVERIFY(!registry.unregisterSurface(first));
    QVERIFY(!registry.encodeKey(first, {
        .key = kodosi::TerminalKey::Enter,
        .action = kodosi::TerminalKeyAction::Press,
    }));
    registry.receiveData({
        subscription,
        1,
        QByteArrayLiteral(" active"),
    });
    QCOMPARE(firstFrames, 1);
    QCOMPARE(secondFrames, 2);
    QVERIFY(registry.unregisterSurface(second));
}

void TerminalKernelTest::registryRestoresOpenSurfacesFromMovedSnapshot()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("snapshot-registry"),
        QStringLiteral("snapshot-subscription"),
        7,
    };
    const kodosi::TerminalSurfaceIdentity first {
        .subscription = subscription,
        .surfaceGeneration = 1,
    };
    const kodosi::TerminalSurfaceIdentity second {
        .subscription = subscription,
        .surfaceGeneration = 2,
    };
    const kodosi::TerminalSurfaceIdentity third {
        .subscription = subscription,
        .surfaceGeneration = 3,
    };
    kodosi::GhosttyTerminalKernel::Frame firstFrame;
    kodosi::GhosttyTerminalKernel::Frame secondFrame;
    int firstFrames = 0;
    int thirdFrames = 0;
    int failures = 0;
    kodosi::TerminalSessionRegistry registry;
    QVERIFY(registry.registerSurface(
        first,
        {
            .frameChanged = [&](auto frame) {
                firstFrame = std::move(frame);
                ++firstFrames;
            },
            .failed = [&](auto) { ++failures; },
        }));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        3,
        24,
        checkpointFor(QByteArrayLiteral("first"), 24, 3),
    }));
    registry.receiveData({subscription, 1, QByteArrayLiteral(" view")});
    QCOMPARE(firstFrames, 2);

    const auto secondRegistration = registry.registerSurface(
        second,
        {
            .frameChanged = [&](auto frame) { secondFrame = std::move(frame); },
            .failed = [&](auto) { ++failures; },
        });
    QVERIFY(secondRegistration);
    QVERIFY(secondRegistration->requiresRefresh);

    QVERIFY(registry.installCheckpoint({
        subscription,
        5,
        3,
        24,
        checkpointFor(QByteArrayLiteral("moved"), 24, 3),
    }));
    QCOMPARE(firstFrames, 3);
    QVERIFY(firstFrame);
    QVERIFY(secondFrame);
    QCOMPARE(firstFrame->nextSequence, std::uint64_t {5});
    QCOMPARE(secondFrame->nextSequence, std::uint64_t {5});
    QVERIFY(frameText(*firstFrame).startsWith(QStringLiteral("moved")));

    registry.receiveData({subscription, 5, QByteArrayLiteral(" on")});
    QCOMPARE(failures, 0);
    QCOMPARE(firstFrames, 4);
    QVERIFY(frameText(*firstFrame).startsWith(QStringLiteral("moved on")));
    QVERIFY(frameText(*secondFrame).startsWith(QStringLiteral("moved on")));

    const auto thirdRegistration = registry.registerSurface(
        third,
        {
            .frameChanged = [&](auto) { ++thirdFrames; },
            .failed = [&](auto) { ++failures; },
        });
    QVERIFY(thirdRegistration);
    QVERIFY(thirdRegistration->requiresRefresh);
    QVERIFY(registry.installCheckpoint({
        subscription,
        6,
        3,
        24,
        checkpointFor(QByteArrayLiteral("moved on"), 24, 3),
    }));
    QCOMPARE(thirdFrames, 1);
    QCOMPARE(firstFrames, 4);
    QCOMPARE(failures, 0);

    QVERIFY(!registry.unregisterSurface(first));
    QVERIFY(!registry.unregisterSurface(second));
    QVERIFY(registry.unregisterSurface(third));
}

void TerminalKernelTest::registryConsumesNormativeControlWireWithoutLosingU64Precision()
{
    constexpr std::uint64_t sequence = 9'007'199'254'740'993ULL;
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        4,
    };
    kodosi::GhosttyTerminalKernel::Frame currentFrame;
    std::optional<kodosi::TerminalFocusOutcome> focusOutcome;
    std::optional<kodosi::TerminalResizeOutcome> resizeOutcome;
    std::optional<kodosi::GhosttyTerminalKernel::Failure> failure;
    bool closed = false;
    kodosi::TerminalSessionRegistry registry;
    QVERIFY(registry.registerSurface(
        {subscription, sequence},
        {
            .frameChanged = [&](auto frame) { currentFrame = std::move(frame); },
            .failed = [&](auto value) { failure = std::move(value); },
            .focusCompleted = [&](auto outcome) {
                focusOutcome = std::move(outcome);
            },
            .resizeCompleted = [&](auto outcome) {
                resizeOutcome = std::move(outcome);
            },
            .closed = [&] { closed = true; },
        }));
    QVERIFY(registry.installCheckpoint({
        subscription,
        sequence,
        2,
        12,
        checkpointFor(QByteArrayLiteral("control"), 12, 2),
    }));

    registry.receiveControl({
        subscription,
        QByteArrayLiteral(
            R"({"type":"term.resize","rows":4,"cols":24,"cellWidthPixels":9,"cellHeightPixels":18,"atSequence":9007199254740993})"),
    });
    QVERIFY(currentFrame);
    QCOMPARE(currentFrame->rows, 4);
    QCOMPARE(currentFrame->columns, 24);
    QCOMPARE(currentFrame->nextSequence, sequence);

    registry.receiveControl({
        subscription,
        QByteArrayLiteral(
            R"({"type":"term.focusApplied","sessionId":"session","requestId":"focus-request","runtimeIncarnationId":"incarnation"})"),
    });
    QVERIFY(focusOutcome);
    QVERIFY(focusOutcome->applied);
    QCOMPARE(focusOutcome->requestId, QStringLiteral("focus-request"));

    registry.receiveControl({
        subscription,
        QByteArrayLiteral(
            R"({"type":"term.resizeApplied","sessionId":"session","requestId":"resize-request","expectedRuntimeIncarnationId":"incarnation","subscriptionId":"subscription","subscriptionGeneration":4,"surfaceGeneration":9007199254740993,"cols":24,"rows":4,"widthPixels":216,"heightPixels":72,"cellWidthPixels":9,"cellHeightPixels":18})"),
    });
    QVERIFY(resizeOutcome);
    QVERIFY(resizeOutcome->applied);
    QCOMPARE(resizeOutcome->surfaceGeneration, sequence);
    QCOMPARE(resizeOutcome->widthPixels, 216U);

    registry.receiveControl({
        subscription,
        QByteArrayLiteral(
            R"({"type":"term.closed","reason":"session ended","finalSequence":9007199254740993})"),
    });
    QVERIFY(closed);
}

void TerminalKernelTest::failedCheckpointPreservesPriorFrame()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        1,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto accepted = kernel.installCheckpoint({
        subscription,
        4,
        2,
        12,
        checkpointFor(QByteArrayLiteral("safe"), 12, 2),
    });
    QVERIFY(accepted);
    const auto prior = kernel.frame();

    auto invalid = QByteArrayLiteral("{\"schemaVersion\":2}");
    const auto rejected = kernel.installCheckpoint({
        subscription,
        99,
        2,
        12,
        invalid,
    });
    QVERIFY(!rejected);
    QCOMPARE(kernel.frame(), prior);
    QCOMPARE(kernel.frame()->nextSequence, 4);
}

void TerminalKernelTest::resizeRequiresExactSequenceBoundary()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        5,
        2,
        12,
        checkpointFor(QByteArrayLiteral("resize"), 12, 2),
    }));

    const auto future = kernel.applyResize(subscription, 6, 4, 24, 9, 18);
    QVERIFY(!future);
    QCOMPARE(
        future.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::SequenceMismatch);

    const auto resized = kernel.applyResize(subscription, 5, 4, 24, 9, 18);
    QVERIFY(resized);
    QCOMPARE((*resized)->rows, 4);
    QCOMPARE((*resized)->columns, 24);
    QCOMPARE((*resized)->nextSequence, 5);
}

void TerminalKernelTest::synchronizedOutputShowsOnlyFinishedFrames()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        2,
        20,
        checkpointFor(QByteArrayLiteral("finished"), 20, 2),
    }));

    const auto begun = kernel.applyData({
        subscription,
        1,
        QByteArrayLiteral("\x1b[?2026h\x1b[2J\x1b[Hhalf"),
    });
    QVERIFY(begun);
    QVERIFY(frameText(**begun).contains(QStringLiteral("finished")));
    QVERIFY(!frameText(**begun).contains(QStringLiteral("half")));

    const auto during = kernel.applyData({subscription, 2, QByteArrayLiteral(" drawn")});
    QVERIFY(during);
    QCOMPARE(*during, *begun);
    QCOMPARE(kernel.nextSequence(), std::optional<std::uint64_t> {3});

    const auto ended = kernel.applyData({subscription, 3, QByteArrayLiteral("\x1b[?2026l")});
    QVERIFY(ended);
    QVERIFY(frameText(**ended).contains(QStringLiteral("half drawn")));
    QVERIFY(!frameText(**ended).contains(QStringLiteral("finished")));

    const auto whole = kernel.applyData({
        subscription,
        4,
        QByteArrayLiteral("\x1b[?2026h\x1b[2J\x1b[Hone write\x1b[?2026l"),
    });
    QVERIFY(whole);
    QVERIFY(frameText(**whole).contains(QStringLiteral("one write")));
}

void TerminalKernelTest::kittyImagesCoverTheHostsCells()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        4,
        10,
        checkpointFor(QByteArrayLiteral("images"), 10, 4),
    }));
    QVERIFY(kernel.applyResize(subscription, 1, 4, 10, 2, 4));
    QByteArray red;
    for (auto pixel = 0; pixel < 4 * 8; ++pixel) {
        red += QByteArray::fromHex("ff0000");
    }

    const auto placed = kernel.applyData({
        subscription,
        1,
        "\x1b[2;4H\x1b_Ga=T,f=24,s=4,v=8,i=7,q=2;" + red.toBase64()
            + "\x1b\\\x1b[1;1H\x1b_Ga=p,i=7,p=2,z=-1,X=1,q=2\x1b\\",
    });
    QVERIFY(placed);
    const auto& images = (*placed)->images;
    QCOMPARE(images.size(), 2);
    QCOMPARE(images[0].bounds, QRectF(0.5, 0, 2, 2));
    QCOMPARE(images[0].layer, kodosi::TerminalImagePlacement::Layer::BelowText);
    QCOMPARE(images[1].bounds, QRectF(3, 1, 2, 2));
    QCOMPARE(images[1].layer, kodosi::TerminalImagePlacement::Layer::AboveText);
    QCOMPARE(images[1].source, QRect(0, 0, 4, 8));
    QCOMPARE(images[1].pixels.pixelColor(1, 1), QColor(Qt::red));
    QCOMPARE(images[0].pixels.cacheKey(), images[1].pixels.cacheKey());

    const auto scrolled = kernel.applyData({subscription, 2, QByteArrayLiteral("\x1b[4;1H\n\n\n\n")});
    QVERIFY(scrolled);
    QVERIFY((*scrolled)->images.isEmpty());
}

void TerminalKernelTest::kittyAnimationsPlayTheirFrames()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        4,
        10,
        checkpointFor(QByteArrayLiteral("animation"), 10, 4),
    }));
    QVERIFY(kernel.applyResize(subscription, 1, 4, 10, 2, 4));
    const auto pixels = [](const char* hex) {
        QByteArray color;
        for (auto pixel = 0; pixel < 4 * 8; ++pixel) {
            color += QByteArray::fromHex(hex);
        }
        return color.toBase64();
    };

    const auto playing = kernel.applyData({
        subscription,
        1,
        "\x1b_Ga=T,f=24,s=4,v=8,i=7,q=2;" + pixels("ff0000") + "\x1b\\"
            + "\x1b_Ga=f,i=7,f=24,s=4,v=8,z=40,q=2;" + pixels("0000ff") + "\x1b\\"
            + "\x1b_Ga=a,i=7,r=1,z=40,q=2\x1b\\\x1b_Ga=a,i=7,s=3,v=1,q=2\x1b\\",
    });
    QVERIFY(playing);
    QCOMPARE((*playing)->images.size(), 1);
    QCOMPARE((*playing)->images.front().pixels.pixelColor(0, 0), QColor(Qt::red));
    QVERIFY((*playing)->imageAnimationDue);

    QTest::qWait(60);
    const auto next = kernel.advanceImageAnimations(subscription);
    QVERIFY(next);
    QCOMPARE((*next)->images.size(), 1);
    QCOMPARE((*next)->images.front().pixels.pixelColor(0, 0), QColor(Qt::blue));
    QVERIFY((*next)->imageAnimationDue);
}

void TerminalKernelTest::checkpointsCarryImagesWithinTheirPixelBudget()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    const auto snapshot = imageCheckpoint();
    const auto visible = visiblePart(snapshot);
    QVERIFY(visible.size() < snapshot.size());
    for (const auto& checkpoint : {snapshot, visible}) {
        kodosi::GhosttyTerminalKernel complete;
        const auto restored = complete.installCheckpoint({subscription, 1, 4, 10, checkpoint});
        QVERIFY(restored);
        QCOMPARE((*restored)->images.size(), 1);
        QCOMPARE((*restored)->images.front().bounds, QRectF(3, 1, 2, 2));
        QCOMPARE((*restored)->images.front().pixels.pixelColor(0, 0), QColor(Qt::red));
    }

    kodosi::GhosttyTerminalKernel complete;
    const auto restored = complete.installCheckpoint({subscription, 1, 4, 10, snapshot});
    QVERIFY(restored);
    kodosi::GhosttyTerminalKernel pendingKernel;
    const auto pending =
        pendingKernel.installCheckpoint({subscription, 1, 4, 10, withoutPixels(snapshot)});
    QVERIFY(pending);
    QVERIFY((*pending)->images.isEmpty());
    QCOMPARE((*pending)->cursor.row, (*restored)->cursor.row);
}

void TerminalKernelTest::placeholderCellsShowTheirPartOfTheImage()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        4,
        10,
        checkpointFor(QByteArrayLiteral("placeholders"), 10, 4),
    }));
    QVERIFY(kernel.applyResize(subscription, 1, 4, 10, 2, 4));

    QByteArray pixels;
    for (auto y = 0; y < 8; ++y) {
        for (auto x = 0; x < 8; ++x) {
            pixels += QByteArray::fromHex(x < 4 ? "ff0000" : "0000ff");
        }
    }
    const auto cells = [](const char32_t* text) {
        return QString::fromUcs4(text).toUtf8();
    };
    const auto shown = kernel.applyData({
        subscription,
        1,
        "\x1b_Ga=T,U=1,f=24,s=8,v=8,i=16777223,p=3,c=2,r=1,q=2;" + pixels.toBase64() + "\x1b\\"
            + "\x1b[2;4H\x1b[38;5;7m\x1b[58;5;3m"
            + cells(U"\U0010EEEE\u0305\u0305\u030D\U0010EEEE")
            + "\x1b[3;7H" + cells(U"\U0010EEEE\u0305\u030D\u030D")
            + "\x1b[4;1H\x1b[38;5;9m\x1b[59m" + cells(U"\U0010EEEE\u0305\u0305") + "\x1b[m",
    });
    QVERIFY(shown);
    const auto& images = (*shown)->images;
    QCOMPARE(images.size(), 2);
    QCOMPARE(images[0].bounds, QRectF(3, 1, 2, 1));
    QCOMPARE(images[0].source, QRect(0, 0, 8, 8));
    QCOMPARE(images[0].layer, kodosi::TerminalImagePlacement::Layer::BelowText);
    QCOMPARE(images[1].bounds, QRectF(6, 2, 1, 1));
    QCOMPARE(images[1].source, QRect(4, 0, 4, 8));
    QCOMPARE(images[0].pixels.cacheKey(), images[1].pixels.cacheKey());

    auto font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    font.setPixelSize(14);
    const auto rendered = kodosi::TerminalRasterizer::render(**shown, font, 1.0);
    const auto cell = kodosi::TerminalRasterizer::cellSize(font);
    const auto center = [&](const int column, const int row) {
        return rendered.pixelColor(QPoint(
            static_cast<int>(cell.width() * (column + 0.5)),
            static_cast<int>(cell.height() * (row + 0.5))));
    };
    QCOMPARE(center(3, 1), QColor(Qt::red));
    QCOMPARE(center(4, 1), QColor(Qt::blue));
    QCOMPARE(center(6, 2), QColor(Qt::blue));
    QCOMPARE(center(0, 3), (*shown)->background);
}

void TerminalKernelTest::synchronizedOutputThatNeverEndsShowsAfterOneSecond()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        2,
        20,
        checkpointFor(QByteArrayLiteral("finished"), 20, 2),
    }));
    QVERIFY(kernel.applyData({
        subscription,
        1,
        QByteArrayLiteral("\x1b[?2026h\x1b[2J\x1b[Hstuck"),
    }));

    QThread::msleep(1'050);
    const auto later = kernel.applyData({subscription, 2, QByteArrayLiteral(" later")});
    QVERIFY(later);
    QVERIFY(frameText(**later).contains(QStringLiteral("stuck later")));

    const auto next = kernel.applyData({subscription, 3, QByteArrayLiteral("!")});
    QVERIFY(next);
    QVERIFY(frameText(**next).contains(QStringLiteral("stuck later!")));
}

void TerminalKernelTest::promptJumpsMoveBetweenMarkedPrompts()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    kodosi::GhosttyTerminalKernel unmarked;
    QVERIFY(unmarked.installCheckpoint({
        subscription,
        1,
        4,
        20,
        checkpointFor(QByteArrayLiteral("start"), 20, 4),
    }));
    QByteArray plain;
    for (int line = 0; line < 20; ++line) {
        plain += "plain-" + QByteArray::number(line) + "\r\n";
    }
    const auto plainFrame = unmarked.applyData({subscription, 1, plain});
    QVERIFY(plainFrame);
    const auto unmoved = unmarked.jumpToPrompt(subscription, -1);
    QVERIFY(unmoved);
    QCOMPARE(*unmoved, *plainFrame);

    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        4,
        20,
        checkpointFor(QByteArrayLiteral("start"), 20, 4),
    }));
    const auto bottom = kernel.applyData({subscription, 1, markedPrompts(6)});
    QVERIFY(bottom);
    QVERIFY(frameText(**bottom).startsWith(QStringLiteral("$ cmd-5")));

    const auto previous = kernel.jumpToPrompt(subscription, -1);
    QVERIFY(previous);
    QVERIFY(frameText(**previous).startsWith(QStringLiteral("$ cmd-4")));
    const auto earlier = kernel.jumpToPrompt(subscription, -2);
    QVERIFY(earlier);
    QVERIFY(frameText(**earlier).startsWith(QStringLiteral("$ cmd-2")));
    const auto later = kernel.jumpToPrompt(subscription, 1);
    QVERIFY(later);
    QVERIFY(frameText(**later).startsWith(QStringLiteral("$ cmd-3")));
}

void TerminalKernelTest::promptJumpKeysMoveTheViewWithoutTerminalInput()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("prompt-view"),
        QStringLiteral("prompt-view-subscription"),
        23,
    };
    view.setWidth(400);
    view.setHeight(160);
    view.setTerminalInteraction(true, false);
    QVERIFY(view.attach(
        registry,
        dispatcher,
        subscription,
        QStringLiteral("prompt-view-incarnation")));
    QVERIFY(registry.installCheckpoint({
        subscription,
        1,
        4,
        20,
        checkpointFor(markedPrompts(6), 20, 4),
    }));
    registry.receiveConnectResult({subscription, ffiOk});
    QCoreApplication::processEvents();
    QVERIFY(view.accessibleText().startsWith(QStringLiteral("$ cmd-5")));

    QKeyEvent up(QEvent::KeyPress, Qt::Key_Up, Qt::ControlModifier | Qt::ShiftModifier);
    QCoreApplication::sendEvent(&view, &up);
    QTRY_VERIFY_WITH_TIMEOUT(view.accessibleText().startsWith(QStringLiteral("$ cmd-4")), 250);
    QKeyEvent down(QEvent::KeyPress, Qt::Key_Down, Qt::ControlModifier | Qt::ShiftModifier);
    QCoreApplication::sendEvent(&view, &down);
    QTRY_VERIFY_WITH_TIMEOUT(view.accessibleText().startsWith(QStringLiteral("$ cmd-5")), 250);
    QCOMPARE(dispatcher.inputCommands.size(), 0);
}

void TerminalKernelTest::historyCompressesAndKeepsItsText()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        2,
    };
    QByteArray restored;
    for (int line = 0; line < 3'000; ++line) {
        restored += "restored-" + QByteArray::number(line).rightJustified(4, '0') + "\r\n";
    }
    kodosi::GhosttyTerminalKernel kernel;
    QVERIFY(kernel.installCheckpoint({
        subscription,
        1,
        4,
        40,
        checkpointFor(restored, 40, 4),
    }));
    const auto afterRestore = kernel.historyMemory();
    if (!afterRestore.compressionSupported) {
        QSKIP("This platform cannot compress terminal history.");
    }
    QVERIFY(afterRestore.compressedPages > 0);

    QByteArray live;
    for (int line = 0; line < 3'000; ++line) {
        live += "live-" + QByteArray::number(line).rightJustified(4, '0') + "\r\n";
    }
    QVERIFY(kernel.applyData({subscription, 1, live}));
    const auto beforeIdle = kernel.historyMemory();
    while (kernel.compressHistory()) {
    }
    const auto afterIdle = kernel.historyMemory();
    QVERIFY(afterIdle.compressedPages > beforeIdle.compressedPages);
    QVERIFY(afterIdle.residentBytes < beforeIdle.residentBytes);

    const auto top = kernel.scrollViewport(subscription, -100'000);
    QVERIFY(top);
    QVERIFY(frameText(**top).contains(QStringLiteral("restored-")));
}

void TerminalKernelTest::consoleCheckpointKeepsHistoryOnResize()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        3,
    };
    QByteArray lines;
    for (int line = 0; line < 8; ++line) {
        lines += QByteArrayLiteral("line ") + QByteArray::number(line) + QByteArrayLiteral("\r\n");
    }
    const auto snapshot = checkpointFor(lines, 12, 3);
    for (const bool console : { false, true }) {
        kodosi::GhosttyTerminalKernel kernel;
        QVERIFY(kernel.installCheckpoint({ subscription, 1, 3, 12, snapshot, console }));

        const auto resized = kernel.applyResize(subscription, 1, 6, 12, 0, 0);

        QVERIFY(resized);
        QCOMPARE(frameText(**resized).contains(QStringLiteral("line 3")), !console);
    }
}

QTEST_MAIN(TerminalKernelTest)

#include "tst_terminal_kernel.moc"
