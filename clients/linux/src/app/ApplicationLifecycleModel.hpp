#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QObject>
#include <QString>

#include <cstdint>
#include <functional>

namespace kodosi {

class ApplicationLifecycleModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY stateChanged)
    Q_PROPERTY(
        quint64 runtimeGeneration
        READ runtimeGeneration
        NOTIFY runtimeGenerationChanged)

public:
    enum class State {
        Starting,
        Ready,
        Failed,
    };
    Q_ENUM(State)

    using Completion = std::function<void(RuntimeBridge::Result)>;
    using StartOperation = std::function<void(Completion)>;
    using StopOperation = std::function<void()>;

    explicit ApplicationLifecycleModel(
        RuntimeBridge& runtime,
        QObject* parent = nullptr);
    ApplicationLifecycleModel(
        RuntimeBridge& runtime,
        StartOperation startOperation,
        StopOperation stopOperation,
        QObject* parent = nullptr);

    [[nodiscard]] State state() const noexcept;
    [[nodiscard]] QString errorText() const;
    [[nodiscard]] quint64 runtimeGeneration() const noexcept;

    void scheduleInitialStart();
    void stop() noexcept;

    Q_INVOKABLE void retry();

signals:
    void stateChanged();
    void runtimeGenerationChanged();
    void runtimeGenerationReady(quint64 generation);

private:
    static constexpr qsizetype MaximumErrorTextLength = 320;
    static constexpr int StartDelayMilliseconds = 16;

    RuntimeBridge& m_runtime;
    StartOperation m_startOperation;
    StopOperation m_stopOperation;
    QString m_errorText;
    State m_state = State::Starting;
    std::uint64_t m_attemptToken = 0;
    quint64 m_runtimeGeneration = 0;
    bool m_initialStartScheduled = false;
    bool m_attemptInFlight = false;
    bool m_stopping = false;

    void beginAttempt();
    void executeAttempt(std::uint64_t token);
    void completeAttempt(
        std::uint64_t token,
        RuntimeBridge::Result result);
    void handleRuntimeRunningChanged(bool running);
    void fail(QString message);

    [[nodiscard]] static QString safeErrorText(
        const RuntimeFailure& failure);
    [[nodiscard]] static QString boundedText(
        QString text,
        QString fallback);
};

}
