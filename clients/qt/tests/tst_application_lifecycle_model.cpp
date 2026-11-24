#include "app/ApplicationLifecycleModel.hpp"

#include <QSignalSpy>
#include <QTest>

#include <functional>
#include <optional>
#include <utility>

class ApplicationLifecycleModelTest final : public QObject {
    Q_OBJECT

private slots:
    void anOutdatedHostRestartsOnlyWhenItRunsInTheBackground();
    void aLaunchOfANewerVersionOffersTheRestart();
};

void ApplicationLifecycleModelTest::anOutdatedHostRestartsOnlyWhenItRunsInTheBackground()
{
    using State = kodosi::ApplicationLifecycleModel::State;
    kodosi::RuntimeBridge runtime;
    auto starts = 0;
    std::optional<kodosi::OutdatedHost> outdated = kodosi::OutdatedHost { .foreground = true, .pid = 7, .terminals = 2 };
    std::function<void(kodosi::HostStop)> stopping;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&](kodosi::ApplicationLifecycleModel::Completion completion) {
            ++starts;
            if (outdated) {
                completion(std::unexpected(
                    kodosi::RuntimeFailure { .ffiResult = -1, .message = QStringLiteral("old"), .outdatedHost = outdated }));
            } else {
                completion(kodosi::RuntimeBridge::Result {});
            }
        },
        [] {},
        [&](std::function<void(kodosi::HostStop)> completion) { stopping = std::move(completion); });

    model.beginInitialStart();
    QTRY_COMPARE(model.state(), State::Failed);
    QVERIFY(model.hostOutdated());
    QVERIFY(model.hostForeground());
    QVERIFY(model.errorText().contains(QStringLiteral("process 7")));
    model.restartHost();
    QVERIFY(!stopping);

    outdated->foreground = false;
    model.retry();
    QTRY_COMPARE(starts, 2);
    QCOMPARE(model.state(), State::Failed);
    QCOMPARE(model.hostTerminals(), 2);
    model.restartHost();
    QCOMPARE(model.state(), State::Starting);
    QVERIFY(stopping);
    std::exchange(stopping, {})(kodosi::HostStop::Refused);
    QCOMPARE(model.state(), State::Failed);
    QCOMPARE(model.errorText(), QStringLiteral("The earlier Kodosi did not stop."));

    outdated.reset();
    model.restartHost();
    QVERIFY(stopping);
    stopping(kodosi::HostStop::Stopped);
    QCOMPARE(model.state(), State::Ready);
    QVERIFY(!model.hostOutdated());
    QCOMPARE(starts, 3);
}

void ApplicationLifecycleModelTest::aLaunchOfANewerVersionOffersTheRestart()
{
    kodosi::RuntimeBridge runtime;
    kodosi::ApplicationLifecycleModel model(runtime, [](kodosi::ApplicationLifecycleModel::Completion) {}, [] {});
    QSignalSpy offered(&model, &kodosi::ApplicationLifecycleModel::restartOffered);

    model.offerRestart(QStringLiteral("0.2.1"), QStringLiteral("0.2.2"));
    model.offerRestart(QString {}, QStringLiteral("0.2.2"));
    QVERIFY(!model.updateInstalled());
    QCOMPARE(offered.count(), 0);
    model.offerRestart(QStringLiteral("0.2.10"), QStringLiteral("0.2.2"));
    QVERIFY(model.updateInstalled());
    QCOMPARE(offered.count(), 1);
    model.offerRestart(QString {}, QStringLiteral("0.2.2"));
    QCOMPARE(offered.count(), 2);
}

QTEST_GUILESS_MAIN(ApplicationLifecycleModelTest)

#include "tst_application_lifecycle_model.moc"
