#include "terminal/GhosttyTerminalKernel.hpp"
#include "presentation/Link.hpp"
#include "terminal/TerminalLink.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include "terminal/TerminalView.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "SessionFixture.hpp"

#include <QByteArray>
#include <QClipboard>
#include <QDir>
#include <QDesktopServices>
#include <QFile>
#include <QFileInfo>
#include <QFocusEvent>
#include <QInputMethodEvent>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QUrl>
#include <QtTest/QTest>

#include <cstdint>
#include <functional>
#include <ghostty/vt.h>
#include <optional>
#include <utility>
#include <vector>

namespace {

constexpr std::int32_t ffiOk = 0;
constexpr std::int32_t ffiBusy = 6;

class FakeTerminalDispatcher final
    : public kodosi::TerminalCommandDispatcher {
public:
    bool running = true;
    int disconnectCount = 0;
    int busyTerminalCommands = 0;
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
        return {};
    }

    Result connectTerminal(
        const kodosi::TerminalSubscription&) override
    {
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
        connectedSubscriptions.append(subscription);
        return {};
    }

    Result refreshTerminal(
        const kodosi::TerminalSubscription& subscription) override
    {
        ++refreshCount;
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
        const kodosi::TerminalSubscription&) override
    {
        return {};
    }

    Result sendTerminalInput(
        const kodosi::TerminalSubscription&,
        const QString&,
        QByteArrayView) override
    {
        return {};
    }

    int refreshCount = 0;
    int refreshFailures = 0;
    QVector<kodosi::TerminalSubscription> connectedSubscriptions;
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

kodosi::GhosttyTerminalKernel::Result applyData(
    kodosi::GhosttyTerminalKernel& kernel,
    const kodosi::TerminalData& data)
{
    if (const auto written = kernel.writeData(data); !written) {
        return std::unexpected(written.error());
    }
    return kernel.latestFrame();
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
    void checkpointsCarryImagesWithinTheirPixelBudget();
    void historyCompressesAndKeepsItsText();
    void registryConsumesNormativeControlWireWithoutLosingU64Precision();
    void attentionAndUnknownControlsKeepTerminalAvailable();
    void hiddenAndDisabledTerminalRejectsInputAndCancelsRetries();
    void terminalControlsStayBoundedAndFenceReattachment();
    void terminalClosePresentsPendingFinalFrameBeforeDetaching();
    void retiredTerminalCloseCannotDetachReplacement();
    void pasteEncodingUsesRestoredTerminalModes();
    void wheelScrollsGhosttyViewportWithoutChangingSequence();
    void terminalViewGatesDeniedCommandsAndRevocation();
    void terminalViewPastesClipboardThroughBoundedAuthority();
    void terminalProgramsWriteTheClipboardAndTheFocusedViewOfTheirOwnTerminalAnswersReads();
    void terminalProgramsReadTheClipboardOnlyWhenAllowed();
    void terminalViewMailboxRejectsReversedDisplayRevisions();
    void terminalLinksToFilesThatRunCodeDoNotOpen();
    void terminalViewCannotActivateLinkFromUndisplayedFrame();
    void terminalViewRetriesRevocationBlurBeforeRefocus();
    void terminalViewGivesItsFocusBackWithoutTheSizeWhenItsTerminalComesBack();
    void terminalViewSendsItsSizeOnlyWhileItMayResize();
    void terminalViewDefersResizeWhileEffectivelyHidden();
    void surfaceControllerFencesSessionIncarnations();
    void surfaceControllerRetriesFailedSeedRefresh();
    void surfaceControllerTimesOutAcceptedSeedRefresh();
    void registryRetiresAbandonedSeedCapacity();
    void registryRestoresOpenSurfacesFromMovedSnapshot();
};

void TerminalKernelTest::attentionAndUnknownControlsKeepTerminalAvailable()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    kodosi::TerminalView view(m_window.contentItem());
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"), QStringLiteral("subscription"), 1};
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    makeTerminalReady(registry, subscription);
    QSignalSpy errors(&view, &kodosi::TerminalView::terminalError);
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.attention","sessionId":"session"})")});
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.later","sessionId":"session"})")});
    QCoreApplication::processEvents();
    QCOMPARE(errors.count(), 0);
    QVERIFY(view.terminalReady());
    QCOMPARE(dispatcher.disconnectCount, 0);
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.attention","sessionId":"wrong"})")});
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
        registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.bell"})")});
    }
    QCoreApplication::processEvents();
    QCOMPARE(errors.count(), 1);
    QVERIFY(!view.terminalReady());
    QCOMPARE(dispatcher.disconnectCount, 1);
    QVERIFY(view.attach(registry, dispatcher, subscription, QStringLiteral("replacement")));
    makeTerminalReady(registry, subscription);
    registry.receiveControl({subscription, QByteArrayLiteral(R"({"type":"term.bell"})")});
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

void TerminalKernelTest::checkpointPrecedesContiguousRawData()
{
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"),
        QStringLiteral("subscription"),
        7,
    };
    kodosi::GhosttyTerminalKernel kernel;
    const auto beforeCheckpoint = applyData(kernel, {
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

    const auto continued = applyData(kernel, {
        subscription,
        12,
        QByteArrayLiteral(" + raw"),
    });
    QVERIFY(continued);
    QCOMPARE((*continued)->nextSequence, 13);
    QVERIFY(frameText(**continued).contains(QStringLiteral("checkpoint + raw")));

    const auto duplicate = applyData(kernel, {
        subscription,
        12,
        QByteArrayLiteral("duplicate"),
    });
    QVERIFY(!duplicate);
    QCOMPARE(
        duplicate.error().code,
        kodosi::GhosttyTerminalKernel::Failure::Code::SequenceMismatch);
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

void TerminalKernelTest::terminalLinksToFilesThatRunCodeDoNotOpen()
{
    using Action = kodosi::LinkDecision::Action;
    const auto decide = [](const QString& path) { return kodosi::decideLink(QUrl::fromLocalFile(path).toString()); };
    QTemporaryDir folder;
    QVERIFY(folder.isValid());
    const QDir dir(QFileInfo(folder.path()).canonicalFilePath());
    QFile notes(dir.filePath(QStringLiteral("notes.txt")));
    QVERIFY(notes.open(QIODevice::WriteOnly));
    notes.write("notes");
    notes.close();
    QFile launcher(dir.filePath(QStringLiteral("setup.desktop")));
    QVERIFY(launcher.open(QIODevice::WriteOnly));
    launcher.write("[Desktop Entry]\nExec=true\n");
    launcher.close();

    QCOMPARE(decide(notes.fileName()).action, Action::Open);
    QCOMPARE(decide(dir.path()).action, Action::Open);
    const auto launcherDecision = decide(launcher.fileName());
    QCOMPARE(launcherDecision.action, Action::Refuse);
    QCOMPARE(launcherDecision.reason, QStringLiteral("Opening it could run a program."));
#ifndef Q_OS_WIN
    QFile script(dir.filePath(QStringLiteral("build")));
    QVERIFY(script.open(QIODevice::WriteOnly));
    script.write("#!/bin/sh\necho hi\n");
    script.close();
    QVERIFY(script.setPermissions(script.permissions() | QFileDevice::ExeOwner));
    QVERIFY(QFile::link(script.fileName(), dir.filePath(QStringLiteral("readme.txt"))));
    QCOMPARE(decide(script.fileName()).action, Action::Refuse);
    const auto disguised = decide(dir.filePath(QStringLiteral("readme.txt")));
    QCOMPARE(disguised.action, Action::Refuse);
    QCOMPARE(disguised.shown, script.fileName());
#endif
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
    view.setTerminalInteraction(true, false);
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
    const auto cell = view.terminalCellSize() * view.viewportScale();
    const QPointF point(cell.width() * 0.5, cell.height() * 0.5);
    QMouseEvent press(
        QEvent::MouseButtonPress,
        point,
        point,
        Qt::LeftButton,
        Qt::LeftButton,
        Qt::ControlModifier);
    QMouseEvent release(
        QEvent::MouseButtonRelease,
        point,
        point,
        Qt::LeftButton,
        Qt::NoButton,
        Qt::ControlModifier);
    QCoreApplication::sendEvent(&view, &press);
    QCoreApplication::sendEvent(&view, &release);
    QCoreApplication::processEvents();
    QDesktopServices::unsetUrlHandler(QStringLiteral("https"));
    QCOMPARE(capture.values.size(), 0);
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

void TerminalKernelTest::terminalViewGivesItsFocusBackWithoutTheSizeWhenItsTerminalComesBack()
{
    kodosi::TerminalSessionRegistry registry;
    FakeTerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(400, 200);
    kodosi::TerminalView view(window.contentItem());
    view.setSize(QSizeF(400, 200));
    view.setTerminalInteraction(true, true);
    const auto focuses = [&dispatcher] {
        QJsonArray found;
        for (const auto& command : dispatcher.terminalCommands) {
            if (command.value(QStringLiteral("type")).toString() == QStringLiteral("session.focus"))
                found.append(command);
        }
        return found;
    };
    const kodosi::TerminalSubscription first {
        QStringLiteral("returning"),
        QStringLiteral("first"),
        1,
    };
    QVERIFY(view.attach(registry, dispatcher, first, QStringLiteral("incarnation")));
    window.show();
    window.requestActivate();
    view.forceActiveFocus();
    QTRY_VERIFY(window.isActive() && view.hasActiveFocus());
    makeTerminalReady(registry, first);
    QTRY_COMPARE(focuses().size(), 1);
    registry.receiveControl({
        first,
        QJsonDocument(QJsonObject {
            {QStringLiteral("type"), QStringLiteral("term.focusApplied")},
            {QStringLiteral("requestId"),
                focuses().last().toObject().value(QStringLiteral("requestId"))},
            {QStringLiteral("runtimeIncarnationId"), QStringLiteral("incarnation")},
        }).toJson(QJsonDocument::Compact),
    });
    QCoreApplication::processEvents();

    const kodosi::TerminalSubscription second {
        QStringLiteral("returning"),
        QStringLiteral("second"),
        2,
    };
    QVERIFY(view.attach(registry, dispatcher, second, QStringLiteral("next-incarnation")));
    makeTerminalReady(registry, second);
    QTRY_COMPARE(focuses().size(), 2);
    QVERIFY(view.hasActiveFocus());
    QVERIFY(!focuses().first().toObject().contains(QStringLiteral("returning")));
    QCOMPARE(focuses().last().toObject().value(QStringLiteral("returning")).toBool(), true);
    window.hide();
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
    QVERIFY(registry.latestFrame(first));
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
    QVERIFY(registry.latestFrame(first));
    QVERIFY(registry.latestFrame(second));
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
    QVERIFY(applyData(kernel, {subscription, 1, live}));
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
