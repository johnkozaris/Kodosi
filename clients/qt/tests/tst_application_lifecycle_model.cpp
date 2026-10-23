#include "app/ApplicationLifecycleModel.hpp"

#include <QCoreApplication>
#include <QFile>
#include <QSignalSpy>
#include <QTest>

#include <functional>
#include <optional>
#include <utility>
#include <vector>

namespace {

struct ReadyCount {
    int count = 0;
    explicit ReadyCount(kodosi::ApplicationLifecycleModel& model)
    {
        QObject::connect(&model, &kodosi::ApplicationLifecycleModel::stateChanged, &model, [this, &model] {
            if (model.state() == kodosi::ApplicationLifecycleModel::State::Ready)
                ++count;
        });
    }
};

kodosi::RuntimeBridge::Result rejected(QString message)
{
    return std::unexpected(kodosi::RuntimeFailure {
        .ffiResult = -1,
        .message = std::move(message),
    });
}

}

class ApplicationLifecycleModelTest final : public QObject {
    Q_OBJECT

private slots:
    void schedulesInitialStartAndReportsReadyOnce();
    void rejectsConcurrentRetryAndDoubleClick();
    void ignoresStaleCompletionAfterRuntimeReset();
    void boundsAndSanitizesSynchronousFailure();
    void runtimeStopReturnsReadyLifecycleToFailure();
    void explicitStopInvalidatesCompletion();
    void anOutdatedHostRestartsOnlyWhenItRunsInTheBackground();
    void anOutdatedHostWithNoTerminalsRestartsWithoutAsking();
    void anInstalledUpdateAsksOnceForARestart();
    void aLaunchOfANewerVersionOffersTheRestart();
};

void ApplicationLifecycleModelTest::
    schedulesInitialStartAndReportsReadyOnce()
{
    kodosi::RuntimeBridge runtime;
    auto startCalls = 0;
    auto stopCalls = 0;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&startCalls](
            kodosi::ApplicationLifecycleModel::Completion completion) {
            ++startCalls;
            completion(kodosi::RuntimeBridge::Result {});
        },
        [&stopCalls] { ++stopCalls; });
    ReadyCount ready(model);

    model.scheduleInitialStart();
    model.scheduleInitialStart();

    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Starting);
    QCOMPARE(startCalls, 0);
    QTRY_COMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QCOMPARE(startCalls, 1);
    QCOMPARE(ready.count, 1);
    QCOMPARE(stopCalls, 0);
}

void ApplicationLifecycleModelTest::
    rejectsConcurrentRetryAndDoubleClick()
{
    kodosi::RuntimeBridge runtime;
    auto startCalls = 0;
    kodosi::ApplicationLifecycleModel::Completion retryCompletion;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&startCalls, &retryCompletion](
            kodosi::ApplicationLifecycleModel::Completion completion) {
            ++startCalls;
            if (startCalls == 1) {
                completion(rejected(QStringLiteral("first failure")));
                return;
            }
            retryCompletion = std::move(completion);
        },
        [] {});
    ReadyCount ready(model);

    model.scheduleInitialStart();
    QTRY_COMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Failed);

    model.retry();
    model.retry();
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Starting);
    QTRY_COMPARE(startCalls, 2);
    QVERIFY(retryCompletion);

    model.retry();
    QCoreApplication::processEvents();
    QCOMPARE(startCalls, 2);

    retryCompletion(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QCOMPARE(ready.count, 1);
}

void ApplicationLifecycleModelTest::
    ignoresStaleCompletionAfterRuntimeReset()
{
    kodosi::RuntimeBridge runtime;
    std::vector<kodosi::ApplicationLifecycleModel::Completion> completions;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&completions](
            kodosi::ApplicationLifecycleModel::Completion completion) {
            completions.push_back(std::move(completion));
        },
        [] {});
    ReadyCount ready(model);

    model.scheduleInitialStart();
    QTRY_COMPARE(completions.size(), std::size_t {1});
    QVERIFY(QMetaObject::invokeMethod(
        &runtime,
        "runningChanged",
        Qt::DirectConnection,
        Q_ARG(bool, false)));
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Failed);

    model.retry();
    QTRY_COMPARE(completions.size(), std::size_t {2});
    completions.front()(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Starting);
    QCOMPARE(ready.count, 0);

    completions.back()(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QCOMPARE(ready.count, 1);
}

void ApplicationLifecycleModelTest::
    boundsAndSanitizesSynchronousFailure()
{
    kodosi::RuntimeBridge runtime;
    QString unsafe(500, QLatin1Char('x'));
    unsafe.insert(10, QChar(0));
    unsafe.insert(20, QLatin1Char('\n'));
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [unsafe](
            kodosi::ApplicationLifecycleModel::Completion completion) {
            completion(rejected(unsafe));
        },
        [] {});

    model.scheduleInitialStart();
    QTRY_COMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Failed);
    QVERIFY(!model.errorText().isEmpty());
    QVERIFY(model.errorText().size() <= 320);
    QVERIFY(!model.errorText().contains(QChar(0)));
    QVERIFY(!model.errorText().contains(QLatin1Char('\n')));
    QVERIFY(model.errorText().endsWith(QChar(0x2026)));
}

void ApplicationLifecycleModelTest::
    runtimeStopReturnsReadyLifecycleToFailure()
{
    kodosi::RuntimeBridge runtime;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [](kodosi::ApplicationLifecycleModel::Completion completion) {
            completion(kodosi::RuntimeBridge::Result {});
        },
        [] {});
    ReadyCount ready(model);

    model.scheduleInitialStart();
    QTRY_COMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QVERIFY(QMetaObject::invokeMethod(
        &runtime,
        "runningChanged",
        Qt::DirectConnection,
        Q_ARG(bool, false)));
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Failed);
    QVERIFY(model.errorText().contains(QStringLiteral("stopped")));

    model.retry();
    QTRY_COMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QCOMPARE(ready.count, 2);
}

void ApplicationLifecycleModelTest::explicitStopInvalidatesCompletion()
{
    kodosi::RuntimeBridge runtime;
    auto stopCalls = 0;
    kodosi::ApplicationLifecycleModel::Completion completion;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&completion](
            kodosi::ApplicationLifecycleModel::Completion pending) {
            completion = std::move(pending);
        },
        [&stopCalls] { ++stopCalls; });
    ReadyCount ready(model);

    model.scheduleInitialStart();
    QTRY_VERIFY(completion);
    model.stop();
    model.stop();
    QCOMPARE(stopCalls, 1);

    completion(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Starting);
    QCOMPARE(ready.count, 0);
}

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

    model.scheduleInitialStart();
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

void ApplicationLifecycleModelTest::anOutdatedHostWithNoTerminalsRestartsWithoutAsking()
{
    using State = kodosi::ApplicationLifecycleModel::State;
    kodosi::RuntimeBridge runtime;
    std::optional<kodosi::OutdatedHost> outdated = kodosi::OutdatedHost { .foreground = false, .pid = 7, .terminals = 0 };
    auto stops = 0;
    kodosi::ApplicationLifecycleModel model(
        runtime,
        [&](kodosi::ApplicationLifecycleModel::Completion completion) {
            if (outdated) {
                completion(std::unexpected(
                    kodosi::RuntimeFailure { .ffiResult = -1, .message = QStringLiteral("old"), .outdatedHost = outdated }));
            } else {
                completion(kodosi::RuntimeBridge::Result {});
            }
        },
        [] {},
        [&](std::function<void(kodosi::HostStop)> completion) {
            ++stops;
            outdated.reset();
            completion(kodosi::HostStop::Stopped);
        });

    model.scheduleInitialStart();
    QTRY_COMPARE(model.state(), State::Ready);
    QCOMPARE(stops, 1);
    QVERIFY(!model.hostOutdated());
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

void ApplicationLifecycleModelTest::anInstalledUpdateAsksOnceForARestart()
{
    kodosi::RuntimeBridge runtime;
    kodosi::ApplicationLifecycleModel model(runtime, [](kodosi::ApplicationLifecycleModel::Completion) {}, [] {});
    QSignalSpy installed(&model, &kodosi::ApplicationLifecycleModel::updateInstalledChanged);

    model.setUpdateInstalled();
    model.setUpdateInstalled();
    QVERIFY(model.updateInstalled());
    QCOMPARE(installed.count(), 1);
    QVERIFY(!model.restartRequested());
    model.restart();
    QVERIFY(model.restartRequested());
}

QTEST_GUILESS_MAIN(ApplicationLifecycleModelTest)

#include "tst_application_lifecycle_model.moc"
