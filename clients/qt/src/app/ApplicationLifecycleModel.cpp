#include "app/ApplicationLifecycleModel.hpp"

#include <QMetaObject>
#include <QPointer>
#include <QThread>
#include <QTimer>

#include <utility>

namespace kodosi {

ApplicationLifecycleModel::ApplicationLifecycleModel(
    RuntimeBridge& runtime,
    QObject* parent)
    : ApplicationLifecycleModel(
          runtime,
          [&runtime](Completion completion) {
              runtime.startAsync(std::move(completion));
          },
          [&runtime] { runtime.quit(); },
          [&runtime](std::function<void(HostStop)> completion) {
              runtime.stopHostAsync(std::move(completion));
          },
          parent)
{
}

ApplicationLifecycleModel::ApplicationLifecycleModel(
    RuntimeBridge& runtime,
    StartOperation startOperation,
    StopOperation stopOperation,
    StopHostOperation stopHostOperation,
    QObject* parent)
    : QObject(parent)
    , m_runtime(runtime)
    , m_startOperation(std::move(startOperation))
    , m_stopOperation(std::move(stopOperation))
    , m_stopHostOperation(std::move(stopHostOperation))
{
    Q_ASSERT(m_startOperation);
    Q_ASSERT(m_stopOperation);
    connect(
        &m_runtime,
        &RuntimeBridge::runningChanged,
        this,
        &ApplicationLifecycleModel::handleRuntimeRunningChanged);
    connect(&m_runtime, &RuntimeBridge::hostForegroundChanged, this, &ApplicationLifecycleModel::hostForegroundChanged);
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

bool ApplicationLifecycleModel::hostForeground() const noexcept
{
    return m_outdatedHost ? m_outdatedHost->foreground : m_runtime.hostForeground();
}

int ApplicationLifecycleModel::hostTerminals() const noexcept
{
    return m_outdatedHost ? static_cast<int>(m_outdatedHost->terminals) : 0;
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

void ApplicationLifecycleModel::restartHost()
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_state != State::Failed || !m_outdatedHost || m_outdatedHost->foreground || m_attemptInFlight
        || m_stopping || !m_stopHostOperation) {
        return;
    }
    ++m_attemptToken;
    const auto token = m_attemptToken;
    m_attemptInFlight = true;
    m_state = State::Starting;
    m_errorText.clear();
    emit stateChanged();
    QPointer<ApplicationLifecycleModel> guard(this);
    m_stopHostOperation([guard, token](HostStop result) {
        if (guard != nullptr)
            guard->completeHostStop(token, result);
    });
}

void ApplicationLifecycleModel::completeHostStop(const std::uint64_t token, const HostStop result)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (m_stopping || !m_attemptInFlight || token != m_attemptToken) {
        return;
    }
    if (result == HostStop::Stopped) {
        executeAttempt(token);
        return;
    }
    m_attemptInFlight = false;
    fail(result == HostStop::Refused ? tr("The earlier Kodosi did not stop.")
                                     : tr("Kodosi could not reach the earlier Kodosi."));
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
    setOutdatedHost(result ? std::nullopt : result.error().outdatedHost);
    if (!result) {
        fail(m_outdatedHost ? describe(*m_outdatedHost) : result.error().message);
        return;
    }

    m_state = State::Ready;
    m_errorText.clear();
    emit stateChanged();
}

void ApplicationLifecycleModel::handleRuntimeRunningChanged(
    const bool running)
{
    if (running || m_stopping || m_state == State::Failed) {
        return;
    }
    ++m_attemptToken;
    m_attemptInFlight = false;
    fail(QStringLiteral("Kodosi stopped unexpectedly. Try again."));
}

void ApplicationLifecycleModel::setOutdatedHost(std::optional<OutdatedHost> host)
{
    const bool foreground = hostForeground();
    m_outdatedHost = host;
    if (hostForeground() != foreground)
        emit hostForegroundChanged();
}

QString ApplicationLifecycleModel::describe(const OutdatedHost& host)
{
    if (host.foreground) {
        return tr("The `kodosi host` command (process %1) runs the earlier version. "
                  "Stop it in its terminal and start it again.")
            .arg(host.pid);
    }
    return tr("The earlier version of Kodosi still runs %n terminals in the background. "
              "Restart it to use the update. Its terminals end.",
        nullptr, static_cast<int>(host.terminals));
}

void ApplicationLifecycleModel::fail(QString message)
{
    const auto safe = boundedText(
        std::move(message),
        QStringLiteral("Kodosi could not start."));
    if (m_state == State::Failed && m_errorText == safe) {
        return;
    }
    m_state = State::Failed;
    m_errorText = safe;
    emit stateChanged();
}

QString ApplicationLifecycleModel::boundedText(
    QString text,
    QString fallback)
{
    for (auto& character : text) {
        if (character.isSpace() || !character.isPrint())
            character = QLatin1Char(' ');
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
