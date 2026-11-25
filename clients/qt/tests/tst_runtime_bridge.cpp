#include "RuntimeApiFixture.hpp"
#include "SessionFixture.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonDocument>
#include <QRegularExpression>
#include <QSignalSpy>
#include <QThread>
#include <chrono>
#include <condition_variable>
#include <mutex>
#include <optional>
#include <QtTest/QTest>

namespace {
bool started(kodosi::RuntimeBridge& bridge)
{
    bool running = false;
    bridge.startAsync([&](auto result) { running = result.has_value(); });
    return QTest::qWaitFor([&] { return running; });
}
QByteArray pipeEvent(const QString& type, const QString& pipe)
{
    return QJsonDocument(QJsonObject { { QStringLiteral("type"), type }, { QStringLiteral("pipe"), pipe },
                             { QStringLiteral("accountEpoch"), 0 }, { QStringLiteral("accountUserId"), QJsonValue::Null } })
        .toJson(QJsonDocument::Compact);
}
class Reader {
public:
    Reader(kodosi::RuntimeBridge& bridge, const QString& pipe, bool start = true)
        : m_bridge(bridge)
        , m_pipe(pipe)
    {
        m_object->moveToThread(&m_thread);
        QObject::connect(&m_thread, &QThread::finished, m_object, &QObject::deleteLater);
        if (start)
            m_thread.start();
        bridge.route(pipe, *m_object, [this](const QByteArray& event, bool afterDrop) {
            const auto sent = m_bridge.send(QByteArrayLiteral("{\"type\":\"session.list\"}")).has_value();
            std::lock_guard lock(m_mutex);
            m_read.append(QJsonDocument::fromJson(event).object().value(QStringLiteral("type")).toString()
                + (afterDrop ? QStringLiteral(" after a drop") : QString {}));
            m_sent = sent ? runtime_fixture::lastCommand : QByteArray {};
            m_changed.notify_all();
        });
    }
    ~Reader()
    {
        m_bridge.unroute(m_pipe);
        m_thread.quit();
        m_thread.wait();
    }
    void start() { m_thread.start(); }
    QStringList waitFor(qsizetype count)
    {
        std::unique_lock lock(m_mutex);
        m_changed.wait_for(lock, std::chrono::seconds(5), [&] { return m_read.size() >= count; });
        return m_read;
    }
    QByteArray sent()
    {
        std::lock_guard lock(m_mutex);
        return m_sent;
    }

private:
    kodosi::RuntimeBridge& m_bridge;
    QString m_pipe;
    QThread m_thread;
    QObject* m_object = new QObject;
    std::mutex m_mutex;
    std::condition_variable m_changed;
    QStringList m_read;
    QByteArray m_sent;
};
class CheckpointSink final : public kodosi::TerminalEventSink {
public:
    void receiveData(kodosi::TerminalData) noexcept override { }
    void receiveControl(kodosi::TerminalControl) noexcept override { }
    void receiveConnectResult(kodosi::TerminalConnectResult) noexcept override { }
    bool installCheckpoint(kodosi::TerminalCheckpoint checkpoint) noexcept override
    {
        installed = checkpoint.snapshot.size();
        return true;
    }
    qsizetype installed = 0;
};
std::int32_t sendCheckpoint(const QByteArray& snapshot, std::uintptr_t length)
{
    return runtime_fixture::callbacks.on_terminal_checkpoint("session", "view", 1, 0, 24, 80, false,
        reinterpret_cast<const uint8_t*>(snapshot.constData()), length, runtime_fixture::userdata);
}
}

class RuntimeBridgeTest final : public QObject {
    Q_OBJECT
private slots:
    void init()
    {
        runtime_fixture::reset();
        runtime_fixture::allowStart = true;
    }
    void fencesAccountsAndInjectsCommandEnvelope()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy events(&bridge, &kodosi::RuntimeBridge::eventReceived);
        QVERIFY(started(bridge));
        auto event = [](const QString& type, int epoch, const QString& user) {
            return QJsonDocument(
                QJsonObject { { QStringLiteral("type"), type }, { QStringLiteral("accountEpoch"), epoch },
                    { QStringLiteral("accountUserId"), user } })
                .toJson(QJsonDocument::Compact);
        };
        runtime_fixture::sendEvent(event(QStringLiteral("auth.ready"), 1, test::id(1)));
        QTRY_COMPARE(events.size(), 1);
        runtime_fixture::sendEvent(event(QStringLiteral("sessions.snapshot"), 0, test::id(1)));
        runtime_fixture::sendEvent(event(QStringLiteral("sessions.snapshot"), 1, test::id(2)));
        runtime_fixture::sendEvent(event(QStringLiteral("sessions.snapshot"), 2, test::id(2)));
        QCoreApplication::processEvents();
        QCOMPARE(events.size(), 1);
        QVERIFY(bridge.send(QByteArrayLiteral("{\"type\":\"session.list\"}")));
        const auto command = QJsonDocument::fromJson(runtime_fixture::lastCommand).object();
        QCOMPARE(command.value(QStringLiteral("accountUserId")).toString(), test::id(1));
        QCOMPARE(command.value(QStringLiteral("accountEpoch")).toInt(), 1);
        runtime_fixture::sendEvent(event(QStringLiteral("auth.ready"), 2, test::id(2)));
        QTRY_COMPARE(events.size(), 2);
    }
    void stoppedGenerationDropsQueuedCallbacks()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy events(&bridge, &kodosi::RuntimeBridge::eventReceived);
        QVERIFY(started(bridge));
        runtime_fixture::sendEvent(
            QByteArrayLiteral("{\"type\":\"system.ready\",\"accountEpoch\":0,\"accountUserId\":null}"));
        bridge.stop();
        QCoreApplication::processEvents();
        QVERIFY(events.isEmpty());
        QVERIFY(!bridge.send(QByteArrayLiteral("{\"type\":\"session.list\"}")));
    }
    void aFullQueueDropsWhatWaitsAndCatchesUpBeforeTheNewerEvents()
    {
        kodosi::RuntimeBridge bridge;
        QStringList heard;
        connect(&bridge, &kodosi::RuntimeBridge::eventReceived, this,
            [&](const QJsonObject& event) { heard.append(QString::number(event.value(QStringLiteral("number")).toInt())); });
        connect(&bridge, &kodosi::RuntimeBridge::fellBehind, this, [&](const QString& message) { heard.append(message); });
        QVERIFY(started(bridge));
        const auto event = [](int number) {
            return QByteArrayLiteral("{\"type\":\"sessions.snapshot\",\"number\":") + QByteArray::number(number)
                + QByteArrayLiteral(",\"accountEpoch\":0,\"accountUserId\":null}");
        };
        qsizetype dropped = 0;
        for (int number = 0; number <= 4096; ++number)
            dropped += event(number).size();
        QTest::ignoreMessage(QtWarningMsg,
            qPrintable(QStringLiteral("the app fell behind the runtime's events and dropped 4097 of them, %1 bytes, so it "
                                      "asks for its state again")
                    .arg(dropped)));
        for (int number = 0; number < 4099; ++number)
            runtime_fixture::sendEvent(event(number));
        QTRY_COMPARE(heard.size(), 3);
        QCOMPARE(heard, (QStringList { QStringLiteral("Kodosi cannot tell if that worked. Check before you try again."),
                            QStringLiteral("4097"), QStringLiteral("4098") }));
        QCOMPARE(QJsonDocument::fromJson(runtime_fixture::lastCommand).object().value(QStringLiteral("type")).toString(),
            QStringLiteral("app.resync"));
        QVERIFY(bridge.isRunning());
    }
    void aPipeEventGoesToItsReaderWhileTheGuiThreadWaits()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy events(&bridge, &kodosi::RuntimeBridge::eventReceived);
        QVERIFY(started(bridge));
        const auto pipe = test::id(9);
        Reader reader(bridge, pipe);
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.opened"), pipe));
        runtime_fixture::sendEvent(
            QByteArrayLiteral("{\"type\":\"sessions.snapshot\",\"accountEpoch\":0,\"accountUserId\":null}"));
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.frame"), pipe));
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.frame"), test::id(8)));
        QCOMPARE(reader.waitFor(2), (QStringList { QStringLiteral("document.opened"), QStringLiteral("document.frame") }));
        QVERIFY(reader.sent().contains("session.list"));
        QVERIFY(events.isEmpty());
        QTRY_COMPARE(events.size(), 2);
        QCOMPARE(events.at(0).first().toJsonObject().value(QStringLiteral("type")).toString(),
            QStringLiteral("sessions.snapshot"));
        QCOMPARE(events.at(1).first().toJsonObject().value(QStringLiteral("pipe")).toString(), test::id(8));
        bridge.unroute(pipe);
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.frame"), pipe));
        QTRY_COMPARE(events.size(), 3);
    }
    void aFullQueueDropsWhatWaitsForAReaderAndTellsItWithItsNextEvent()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy behind(&bridge, &kodosi::RuntimeBridge::fellBehind);
        QVERIFY(started(bridge));
        const auto pipe = test::id(9);
        Reader reader(bridge, pipe, false);
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.frame"), pipe));
        QTest::ignoreMessage(QtWarningMsg, QRegularExpression(QStringLiteral("^the app fell behind the runtime's events")));
        for (int number = 0; number < 4096; ++number)
            runtime_fixture::sendEvent(
                QByteArrayLiteral("{\"type\":\"sessions.snapshot\",\"accountEpoch\":0,\"accountUserId\":null}"));
        runtime_fixture::sendEvent(pipeEvent(QStringLiteral("document.opened"), pipe));
        reader.start();
        QCOMPARE(reader.waitFor(1), QStringList { QStringLiteral("document.opened after a drop") });
        QTRY_COMPARE(behind.size(), 1);
    }
    void anOutdatedHostRefusesTheStartWithItsFacts()
    {
        runtime_fixture::allowStart = false;
        kodosi_start_failure_t failure { .code = KODOSI_START_HOST_OUTDATED, .host_kind = KODOSI_HOST_KIND_BACKGROUND,
            .host_pid = 42, .host_terminals = 3, .message = "Kodosi was updated." };
        runtime_fixture::startFailure = failure;
        kodosi::RuntimeBridge bridge;
        std::optional<kodosi::RuntimeBridge::Result> result;
        bridge.startAsync([&](auto value) { result = std::move(value); });
        QTRY_VERIFY(result);
        QVERIFY(!*result);
        QCOMPARE(result->error().message, QStringLiteral("Kodosi was updated."));
        QVERIFY(result->error().outdatedHost);
        QVERIFY(!result->error().outdatedHost->foreground);
        QCOMPARE(result->error().outdatedHost->terminals, 3u);
    }
    void quitStopsABackgroundHostButNotAForegroundOne()
    {
        for (const auto& [host, stops] : { std::pair { "background", 1 }, std::pair { "foreground", 0 } }) {
            runtime_fixture::hostStops = 0;
            kodosi::RuntimeBridge bridge;
            QVERIFY(started(bridge));
            runtime_fixture::sendEvent(QByteArrayLiteral("{\"type\":\"system.ready\",\"host\":\"") + host
                + QByteArrayLiteral("\",\"accountEpoch\":0,\"accountUserId\":null}"));
            QTRY_COMPARE(bridge.hostForeground(), stops == 0);
            bridge.quit();
            QVERIFY(!bridge.isRunning());
            QCOMPARE(runtime_fixture::hostStops, stops);
        }
    }
    void installsCheckpointsUpToTheSnapshotLimit()
    {
        CheckpointSink sink;
        kodosi::RuntimeBridge bridge(sink);
        QVERIFY(started(bridge));
        const QByteArray snapshot(9 * 1024 * 1024, 'x');
        QCOMPARE(sendCheckpoint(snapshot, static_cast<std::uintptr_t>(snapshot.size())), KODOSI_FFI_OK);
        QCOMPARE(sink.installed, snapshot.size());
        QCOMPARE(sendCheckpoint(snapshot, std::uintptr_t { KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES } + 1),
            KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED);
        QCOMPARE(sink.installed, snapshot.size());
    }
};
QTEST_GUILESS_MAIN(RuntimeBridgeTest)
#include "tst_runtime_bridge.moc"
