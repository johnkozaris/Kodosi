#include "RuntimeApiFixture.hpp"
#include "SessionFixture.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonDocument>
#include <QSignalSpy>
#include <memory>
#include <optional>
#include <thread>
#include <QtTest/QTest>

namespace {
bool started(kodosi::RuntimeBridge& bridge)
{
    bool running = false;
    bridge.startAsync([&](auto result) { running = result.has_value(); });
    return QTest::qWaitFor([&] { return running; });
}
// A decoder that tells what reached it, and refuses frames once told to.
class Decoder final : public kodosi::VideoSink {
public:
    bool take(const Frame& frame) noexcept override
    {
        auto line = QStringLiteral("%1 %2 %3").arg(frame.codec == KODOSI_VIDEO_HEVC ? QStringLiteral("hevc") : QStringLiteral("h264"),
            frame.key ? QStringLiteral("key") : QStringLiteral("delta"), QString::fromLatin1(frame.data.toByteArray()));
        if (const auto part = frame.region)
            line += QStringLiteral(" of %1,%2 %3x%4").arg(part->x()).arg(part->y()).arg(part->width()).arg(part->height());
        heard.append(line);
        return takes;
    }
    void hand(QByteArrayView message) noexcept override { heard.append(QString::fromUtf8(message.toByteArray())); }
    void repair() noexcept override { heard.append(QStringLiteral("repair")); }

    QStringList heard;
    bool takes = true;
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
    void aVideoPipesFramesReachItsDecoderWhileTheGuiThreadWaitsAndNoneOnceItIsUnwatched()
    {
        kodosi::RuntimeBridge bridge;
        QVERIFY(started(bridge));
        const auto pipe = test::id(9);
        const auto id = pipe.toUtf8();
        const auto other = test::id(8).toUtf8();
        auto decoder = std::make_shared<Decoder>();
        bridge.watch(pipe, decoder);
        QCOMPARE(runtime_fixture::videoCalls, QStringList { QStringLiteral("watch ") + pipe });
        const QByteArray picture("picture");
        const QByteArray state(R"({"width":800})");
        const kodosi_video_region_t part { .x = 1, .y = 2, .width = 3, .height = 4 };
        const auto frame = [&picture](std::uint8_t kind, const kodosi_video_region_t* region) {
            return kodosi_video_frame_t { .codec = KODOSI_VIDEO_HEVC, .kind = kind, .region = region,
                .data = reinterpret_cast<const std::uint8_t*>(picture.constData()),
                .length = static_cast<std::uintptr_t>(picture.size()) };
        };
        const auto key = frame(KODOSI_VIDEO_KEY, &part);
        const auto delta = frame(KODOSI_VIDEO_DELTA, nullptr);
        const auto& callbacks = runtime_fixture::callbacks;
        auto* userdata = runtime_fixture::userdata;
        QList<std::int32_t> answers;
        std::thread runtime([&] {
            answers.append(callbacks.on_video_frame(id.constData(), &key, userdata));
            callbacks.on_video_message(id.constData(), reinterpret_cast<const std::uint8_t*>(state.constData()),
                static_cast<std::uintptr_t>(state.size()), userdata);
            decoder->takes = false;
            answers.append(callbacks.on_video_frame(id.constData(), &delta, userdata));
            answers.append(callbacks.on_video_frame(other.constData(), &delta, userdata));
            callbacks.on_video_repair(id.constData(), userdata);
        });
        runtime.join();
        QCOMPARE(answers, (QList<std::int32_t> { KODOSI_VIDEO_TAKEN, KODOSI_VIDEO_FAILED, KODOSI_VIDEO_TAKEN }));
        QCOMPARE(decoder->heard,
            (QStringList { QStringLiteral("hevc key picture of 1,2 3x4"), QString::fromLatin1(state),
                QStringLiteral("hevc delta picture"), QStringLiteral("repair") }));

        bridge.unwatch(pipe);
        QCOMPARE(runtime_fixture::videoCalls.last(), QStringLiteral("unwatch ") + pipe);
        QCOMPARE(callbacks.on_video_frame(id.constData(), &key, userdata), KODOSI_VIDEO_TAKEN);
        QCOMPARE(decoder->heard.size(), 4);
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
