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
    void aLaunchNamesItsVersion();
    void activationsFromTheRuntimeThreadReachTheAppThread();
};

void AppActivationTest::aLaunchNamesItsVersion()
{
    QCOMPARE(parse({ { QStringLiteral("appVersion"), QStringLiteral("0.2.2") } }).appVersion, QStringLiteral("0.2.2"));
    QVERIFY(parse({ { QStringLiteral("appVersion"), QStringLiteral("0.2.2\nrun") } }).appVersion.isEmpty());
    QVERIFY(parse({ { QStringLiteral("appVersion"), QString(33, QLatin1Char('1')) } }).appVersion.isEmpty());
    QVERIFY(parse({}).appVersion.isEmpty());
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

QTEST_GUILESS_MAIN(AppActivationTest)

#include "tst_app_activation.moc"
