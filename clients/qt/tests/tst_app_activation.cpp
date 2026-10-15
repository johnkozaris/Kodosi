#include "app/AppActivation.hpp"

#include <QJsonDocument>
#include <QJsonObject>
#include <QThread>
#include <QtTest/QTest>

namespace {

kodosi::AppActivation parse(const QJsonObject& object)
{
    return kodosi::AppActivation::fromJson(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

void hand(kodosi::AppActivations& activations, const kodosi::AppActivation& activation)
{
    const auto json = activation.toJson();
    kodosi::AppActivations::receive(
        reinterpret_cast<const uint8_t*>(json.constData()), static_cast<uintptr_t>(json.size()), &activations);
}

}

class AppActivationTest final : public QObject {
    Q_OBJECT

private slots:
    void launchesForwardTheirRouteAndToken();
    void malformedFieldsStillShowTheWindow();
    void activationsWaitForTheirHandler();
    void activationsFromTheRuntimeThreadReachTheAppThread();
    void activationTokensAreTakenOnceAndRestored();
};

void AppActivationTest::launchesForwardTheirRouteAndToken()
{
    const kodosi::AppActivation routed {
        .route = kodosi::DeepLinkRouter::parse(QStringLiteral("kodosi://session/0198aaaa-0000-7000-8000-000000000001")),
        .token = QStringLiteral("xdg-token"),
    };
    QVERIFY(routed.route->destination);
    const auto received = kodosi::AppActivation::fromJson(routed.toJson());
    QCOMPARE(received.token, routed.token);
    QCOMPARE(received.route->destination, routed.route->destination);

    const kodosi::AppActivation invalid {
        .route = kodosi::DeepLinkRouter::parse(QStringLiteral("kodosi://unknown/route")),
        .token = {},
    };
    QVERIFY(!invalid.route->destination);
    const auto rejected = kodosi::AppActivation::fromJson(invalid.toJson());
    QVERIFY(rejected.route && !rejected.route->destination);
    QCOMPARE(rejected.route->error, invalid.route->error);

    const auto plain = kodosi::AppActivation::fromJson(kodosi::AppActivation {}.toJson());
    QVERIFY(!plain.route);
    QVERIFY(plain.token.isEmpty());
    QCOMPARE(kodosi::AppActivation {}.toJson(), QByteArrayLiteral("{}"));
}

void AppActivationTest::malformedFieldsStillShowTheWindow()
{
    for (const auto& object : {
             QJsonObject { { QStringLiteral("token"), QStringLiteral("bad\ntoken") } },
             QJsonObject { { QStringLiteral("token"), QString(kodosi::MaximumActivationTokenBytes + 1, QLatin1Char('t')) } },
             QJsonObject { { QStringLiteral("session"), 42 } },
             QJsonObject { { QStringLiteral("session"), QStringLiteral("bad\u0001session") } },
             QJsonObject { { QStringLiteral("session"), QString(kodosi::DeepLinkRouter::MaximumIdentityCharacters + 1, QLatin1Char('s')) } },
             QJsonObject { { QStringLiteral("routeError"), 0 } },
             QJsonObject { { QStringLiteral("routeError"), 99 } },
             QJsonObject { { QStringLiteral("routeError"), QStringLiteral("1") } },
         }) {
        const auto activation = parse(object);
        QVERIFY(!activation.route);
        QVERIFY(activation.token.isEmpty());
    }
    QVERIFY(!kodosi::AppActivation::fromJson(QByteArrayLiteral("not json")).route);
}

void AppActivationTest::activationsWaitForTheirHandler()
{
    kodosi::AppActivations activations;
    for (int index = 0; index < 20; ++index)
        hand(activations, { .route = {}, .token = QStringLiteral("token-%1").arg(index) });
    QCoreApplication::processEvents();

    QStringList tokens;
    activations.setHandler([&tokens](const kodosi::AppActivation& activation) { tokens.append(activation.token); });
    QCOMPARE(tokens.size(), 16);
    QCOMPARE(tokens.first(), QStringLiteral("token-0"));
    QCOMPARE(tokens.last(), QStringLiteral("token-15"));

    hand(activations, { .route = {}, .token = QStringLiteral("later") });
    QTRY_COMPARE(tokens.last(), QStringLiteral("later"));
}

void AppActivationTest::activationsFromTheRuntimeThreadReachTheAppThread()
{
    kodosi::AppActivations activations;
    QThread* deliveredOn = nullptr;
    activations.setHandler([&deliveredOn](const kodosi::AppActivation&) { deliveredOn = QThread::currentThread(); });
    auto* runtime = QThread::create([&activations] { hand(activations, {}); });
    runtime->start();
    QVERIFY(runtime->wait(5000));
    delete runtime;
    QTRY_COMPARE(deliveredOn, QThread::currentThread());
}

void AppActivationTest::activationTokensAreTakenOnceAndRestored()
{
    qputenv("XDG_ACTIVATION_TOKEN", "launch-token");
    QCOMPARE(kodosi::takeActivationToken(), QStringLiteral("launch-token"));
    QVERIFY(!qEnvironmentVariableIsSet("XDG_ACTIVATION_TOKEN"));
    QVERIFY(kodosi::takeActivationToken().isEmpty());

    qputenv("XDG_ACTIVATION_TOKEN", "unsafe\x01token");
    QVERIFY(kodosi::takeActivationToken().isEmpty());
    QVERIFY(!qEnvironmentVariableIsSet("XDG_ACTIVATION_TOKEN"));

    QString during;
    kodosi::withActivationToken(QStringLiteral("raise-token"), [&during] {
        during = qEnvironmentVariable("XDG_ACTIVATION_TOKEN");
    });
    QCOMPARE(during, QStringLiteral("raise-token"));
    QVERIFY(!qEnvironmentVariableIsSet("XDG_ACTIVATION_TOKEN"));

    bool ran = false;
    kodosi::withActivationToken({}, [&ran] { ran = !qEnvironmentVariableIsSet("XDG_ACTIVATION_TOKEN"); });
    QVERIFY(ran);
}

QTEST_GUILESS_MAIN(AppActivationTest)

#include "tst_app_activation.moc"
