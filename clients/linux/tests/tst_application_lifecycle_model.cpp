#include "app/ApplicationLifecycleModel.hpp"

#include <QCoreApplication>
#include <QFile>
#include <QSignalSpy>
#include <QTest>

#include <utility>
#include <vector>

namespace {

kodosi::RuntimeBridge::Result rejected(QString message)
{
    return std::unexpected(kodosi::RuntimeFailure {
        .code = kodosi::RuntimeFailure::Code::StartRejected,
        .ffiResult = -1,
        .message = std::move(message),
    });
}

}

class ApplicationLifecycleModelTest final : public QObject {
    Q_OBJECT

private slots:
    void schedulesInitialStartAndPublishesOneGeneration();
    void rejectsConcurrentRetryAndDoubleClick();
    void ignoresStaleCompletionAfterRuntimeReset();
    void boundsAndSanitizesSynchronousFailure();
    void runtimeStopReturnsReadyLifecycleToFailure();
    void explicitStopInvalidatesCompletion();
};

void ApplicationLifecycleModelTest::
    schedulesInitialStartAndPublishesOneGeneration()
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
    QSignalSpy ready(
        &model,
        &kodosi::ApplicationLifecycleModel::runtimeGenerationReady);

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
    QCOMPARE(model.runtimeGeneration(), 1);
    QCOMPARE(ready.count(), 1);
    QCOMPARE(ready.constFirst().constFirst().toULongLong(), 1ULL);
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
    QCOMPARE(model.runtimeGeneration(), 1);
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
    QCOMPARE(model.runtimeGeneration(), 0);

    completions.back()(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Ready);
    QCOMPARE(model.runtimeGeneration(), 1);
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
    QCOMPARE(model.runtimeGeneration(), 2);
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

    model.scheduleInitialStart();
    QTRY_VERIFY(completion);
    model.stop();
    model.stop();
    QCOMPARE(stopCalls, 1);

    completion(kodosi::RuntimeBridge::Result {});
    QCOMPARE(
        model.state(),
        kodosi::ApplicationLifecycleModel::State::Starting);
    QCOMPARE(model.runtimeGeneration(), 0);
}

QTEST_GUILESS_MAIN(ApplicationLifecycleModelTest)

#include "tst_application_lifecycle_model.moc"
