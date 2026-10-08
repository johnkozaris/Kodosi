#include "app/ApplicationLifecycleModel.hpp"

#include <QMetaObject>
#include <QPointer>
#include <QThread>
#include <QTimer>

#include <utility>

namespace kodosi {
namespace {

QString fallbackForFailure(const RuntimeFailure::Code code)
{
    switch (code) {
    case RuntimeFailure::Code::ContractMismatch:
        return QStringLiteral(
            "This Kodosi build is not compatible with the installed runtime.");
    case RuntimeFailure::Code::StartRejected:
        return QStringLiteral("The Kodosi runtime rejected startup.");
    case RuntimeFailure::Code::RuntimeStopped:
        return QStringLiteral("The Kodosi runtime stopped unexpectedly.");
    case RuntimeFailure::Code::InvalidArgument:
    case RuntimeFailure::Code::FfiRejected:
        return QStringLiteral("Kodosi could not start its runtime.");
    }
    return QStringLiteral("Kodosi could not start its runtime.");
}

}

ApplicationLifecycleModel::ApplicationLifecycleModel(
    RuntimeBridge& runtime,
    QObject* parent)
    : ApplicationLifecycleModel(
          runtime,
          [&runtime](Completion completion) {
              runtime.startAsync(std::move(completion));
          },
          [&runtime] { runtime.stop(); },
          parent)
{
}

ApplicationLifecycleModel::ApplicationLifecycleModel(
    RuntimeBridge& runtime,
    StartOperation startOperation,
    StopOperation stopOperation,
    QObject* parent)
    : QObject(parent)
    , m_runtime(runtime)
    , m_startOperation(std::move(startOperation))
    , m_stopOperation(std::move(stopOperation))
{
    Q_ASSERT(m_startOperation);
    Q_ASSERT(m_stopOperation);
    connect(
        &m_runtime,
        &RuntimeBridge::runningChanged,
        this,
        &ApplicationLifecycleModel::handleRuntimeRunningChanged);
}

ApplicationLifecycleModel::State
ApplicationLifecycleModel::state() const noexcept
{
    return m_state;
}

QString ApplicationLifecycleModel::errorText() const
{
    return m_errorText;
}

quint64 ApplicationLifecycleModel::runtimeGeneration() const noexcept
{
    return m_runtimeGeneration;
}

void ApplicationLifecycleModel::scheduleInitialStart()
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_initialStartScheduled || m_stopping) {
        return;
    }
    m_initialStartScheduled = true;
    beginAttempt();
}

void ApplicationLifecycleModel::stop() noexcept
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_stopping) {
        return;
    }
    m_stopping = true;
    ++m_attemptToken;
    m_attemptInFlight = false;
    m_stopOperation();
}

void ApplicationLifecycleModel::retry()
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_state != State::Failed || m_attemptInFlight || m_stopping) {
        return;
    }
    beginAttempt();
}

void ApplicationLifecycleModel::beginAttempt()
{
    ++m_attemptToken;
    const auto token = m_attemptToken;
    m_attemptInFlight = true;
    if (m_state != State::Starting || !m_errorText.isEmpty()) {
        m_state = State::Starting;
        m_errorText.clear();
        emit stateChanged();
    }
    QTimer::singleShot(
        StartDelayMilliseconds,
        this,
        [this, token] { executeAttempt(token); });
}

void ApplicationLifecycleModel::executeAttempt(const std::uint64_t token)
{
    if (m_stopping || !m_attemptInFlight || token != m_attemptToken) {
        return;
    }
    QPointer<ApplicationLifecycleModel> guard(this);
    m_startOperation(
        [guard, token](RuntimeBridge::Result result) mutable {
            if (guard == nullptr) {
                return;
            }
            if (guard->thread() == QThread::currentThread()) {
                guard->completeAttempt(token, std::move(result));
                return;
            }
            QMetaObject::invokeMethod(
                guard,
                [guard, token, result = std::move(result)]() mutable {
                    if (guard != nullptr) {
                        guard->completeAttempt(
                            token,
                            std::move(result));
                    }
                },
                Qt::QueuedConnection);
        });
}

void ApplicationLifecycleModel::completeAttempt(
    const std::uint64_t token,
    RuntimeBridge::Result result)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_stopping || !m_attemptInFlight || token != m_attemptToken) {
        return;
    }
    m_attemptInFlight = false;
    if (!result) {
        fail(safeErrorText(result.error()));
        return;
    }

    m_state = State::Ready;
    m_errorText.clear();
    ++m_runtimeGeneration;
    emit stateChanged();
    emit runtimeGenerationChanged();
    emit runtimeGenerationReady(m_runtimeGeneration);
}

void ApplicationLifecycleModel::handleRuntimeRunningChanged(
    const bool running)
{
    if (running || m_stopping || m_state == State::Failed) {
        return;
    }
    ++m_attemptToken;
    m_attemptInFlight = false;
    fail(QStringLiteral(
        "The Kodosi runtime stopped unexpectedly. Try again."));
}

void ApplicationLifecycleModel::fail(QString message)
{
    const auto safe = boundedText(
        std::move(message),
        QStringLiteral("Kodosi could not start its runtime."));
    if (m_state == State::Failed && m_errorText == safe) {
        return;
    }
    m_state = State::Failed;
    m_errorText = safe;
    emit stateChanged();
}

QString ApplicationLifecycleModel::safeErrorText(
    const RuntimeFailure& failure)
{
    return boundedText(
        failure.message,
        fallbackForFailure(failure.code));
}

QString ApplicationLifecycleModel::boundedText(
    QString text,
    QString fallback)
{
    for (auto& character : text) {
        if (character.isSpace()) {
            character = QLatin1Char(' ');
        } else if (!character.isPrint()) {
            character = QLatin1Char(' ');
        }
    }
    text = text.simplified();
    if (text.isEmpty()) {
        text = std::move(fallback);
    }
    if (text.size() <= MaximumErrorTextLength) {
        return text;
    }
    text.truncate(MaximumErrorTextLength - 1);
    if (!text.isEmpty() && text.back().isHighSurrogate()) {
        text.chop(1);
    }
    return text.trimmed() + QChar(0x2026);
}

}
