#include "RuntimeApiFixture.hpp"
#include "SessionFixture.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonDocument>
#include <QSignalSpy>
#include <limits>
#include <optional>
#include <QtTest/QTest>

namespace {
bool started(kodosi::RuntimeBridge& bridge)
{
    bool running = false;
    bridge.startAsync([&](auto result) { running = result.has_value(); });
    return QTest::qWaitFor([&] { return running; });
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
    void requiresExactDesktopProtocol()
    {
        for (const auto version : { 57u, 59u }) {
            runtime_fixture::protocolVersion = version;
            kodosi::RuntimeBridge bridge;
            std::optional<kodosi::RuntimeBridge::Result> result;
            bridge.startAsync([&](auto value) { result = std::move(value); });
            QVERIFY(result && !*result);
            QCOMPARE(result->error().code, kodosi::RuntimeFailure::Code::ContractMismatch);
            QVERIFY(!bridge.isRunning());
            QVERIFY(runtime_fixture::userdata == nullptr);
        }
        runtime_fixture::protocolVersion = 58;
        kodosi::RuntimeBridge bridge;
        QVERIFY(started(bridge));
        QVERIFY(bridge.isRunning());
    }
    void asynchronousStartupCanBeCancelledBeforeInstallation()
    {
        kodosi::RuntimeBridge bridge;
        bool completed = false;
        bridge.startAsync([&](auto) { completed = true; });
        bridge.stop();
        QTest::qWait(100);
        QVERIFY(!bridge.isRunning());
        QVERIFY(!completed);
        bridge.startAsync([&](auto result) { completed = result.has_value(); });
        QTRY_VERIFY(completed);
        QVERIFY(bridge.isRunning());
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
    void boundsSmallQueuedEventsAndReportsOnce()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy errors(&bridge,&kodosi::RuntimeBridge::eventError);
        QVERIFY(started(bridge));
        const auto event=QByteArrayLiteral("{\"type\":\"system.ready\",\"accountEpoch\":0,\"accountUserId\":null}");
        for(int i=0;i<10000;++i)runtime_fixture::sendEvent(event);
        QTRY_VERIFY(!bridge.isRunning());
        QCOMPARE(errors.size(),1);
    }
    void preservesUnsignedIdentitiesAndRejectsCallerEnvelope()
    {
        kodosi::RuntimeBridge bridge;
        QSignalSpy events(&bridge, &kodosi::RuntimeBridge::eventReceived);
        QVERIFY(started(bridge));
        runtime_fixture::sendEvent(QByteArrayLiteral(
            R"({"type":"auth.ready","accountEpoch":18446744073709551615,"accountUserId":"owner"})"));
        QTRY_COMPARE(events.size(), 1);
        QCOMPARE(events.first().at(1).toULongLong(), std::numeric_limits<quint64>::max());
        const auto command = QByteArrayLiteral(
            R"({"type":"session.resize","surfaceGeneration":9223372036854775809,"subscriptionGeneration":18446744073709551615})");
        QVERIFY(bridge.send(command));
        QVERIFY(runtime_fixture::lastCommand.contains("\"accountEpoch\":18446744073709551615"));
        QVERIFY(runtime_fixture::lastCommand.contains("\"surfaceGeneration\":9223372036854775809"));
        QVERIFY(runtime_fixture::lastCommand.contains("\"subscriptionGeneration\":18446744073709551615"));
        const auto nested = QByteArrayLiteral(
            "{\"type\":\"example\",\"nested\":{\"accountEpoch\":1,\"text\":\"braces } , and escaped \\\"quote\\\"\","
            "\"number\":9223372036854775809},\"list\":[1,{\"field\":2}]}");
        QVERIFY(bridge.send(nested));
        QVERIFY(runtime_fixture::lastCommand.contains("\"number\":9223372036854775809"));
        QVERIFY(QJsonDocument::fromJson(runtime_fixture::lastCommand).isObject());
        for (const auto& invalid : {
            QByteArrayLiteral(R"({"type":"session.list","accountEpoch":0})"),
            QByteArrayLiteral(R"({"type":"session.list","accountUserId":null})"),
            QByteArrayLiteral(R"({"type":"session.list","account\u0045poch":0})"),
            QByteArrayLiteral(R"({"type":"session.list","type":"shutdown"})")}) {
            QVERIFY(!bridge.send(invalid));
        }
        runtime_fixture::sendEvent(QByteArrayLiteral(
            R"({"type":"sessions.snapshot","accountEpoch":18446744073709551614,"accountUserId":"owner"})"));
        runtime_fixture::sendEvent(QByteArrayLiteral(
            R"({"type":"auth.ready","accountEpoch":1,"accountEpoch":18446744073709551615,"accountUserId":"owner"})"));
        QCoreApplication::processEvents();
        QCOMPARE(events.size(), 1);
    }
    void admissionErrorsAreNotCompletion()
    {
        kodosi::RuntimeBridge bridge;
        QVERIFY(started(bridge));
        runtime_fixture::commandResult = KODOSI_FFI_BUSY;
        QVERIFY(!bridge.send(QByteArrayLiteral("{\"type\":\"session.list\"}")));
    }
};
QTEST_GUILESS_MAIN(RuntimeBridgeTest)
#include "tst_runtime_bridge.moc"
