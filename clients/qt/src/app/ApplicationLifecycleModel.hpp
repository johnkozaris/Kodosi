#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QObject>
#include <QString>

#include <cstdint>
#include <functional>
#include <optional>

namespace kodosi {

class ApplicationLifecycleModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY stateChanged FINAL)
    Q_PROPERTY(QString errorText READ errorText NOTIFY stateChanged FINAL)
    Q_PROPERTY(bool hostOutdated READ hostOutdated NOTIFY stateChanged FINAL)
    Q_PROPERTY(bool hostForeground READ hostForeground NOTIFY hostForegroundChanged FINAL)
    Q_PROPERTY(int hostTerminals READ hostTerminals NOTIFY stateChanged FINAL)
    Q_PROPERTY(bool updateInstalled READ updateInstalled NOTIFY updateInstalledChanged FINAL)
    Q_PROPERTY(int restartStops READ restartStops NOTIFY restartStopsChanged FINAL)

public:
    enum class State {
        Starting,
        Ready,
        Failed,
    };
    Q_ENUM(State)

    using Completion = std::function<void(RuntimeBridge::Result)>;
    using StartOperation = std::function<void(Completion)>;
    // `update` tells the host that it stops to install an update and comes right back.
    using StopOperation = std::function<void(bool update)>;
    using StopHostOperation = std::function<void(std::function<void(HostStop)>)>;

    explicit ApplicationLifecycleModel(
        RuntimeBridge& runtime,
        QObject* parent = nullptr);
    ApplicationLifecycleModel(
        RuntimeBridge& runtime,
        StartOperation startOperation,
        StopOperation stopOperation,
        StopHostOperation stopHostOperation = {},
        QObject* parent = nullptr);

    [[nodiscard]] State state() const noexcept;
    [[nodiscard]] QString errorText() const;
    [[nodiscard]] bool hostOutdated() const noexcept { return m_outdatedHost.has_value(); }
    [[nodiscard]] bool hostForeground() const noexcept;
    [[nodiscard]] int hostTerminals() const noexcept;
    [[nodiscard]] bool updateInstalled() const noexcept { return m_updateInstalled; }
    [[nodiscard]] bool restartRequested() const noexcept { return m_restartRequested; }
    // Kodosi restarted by itself to install an update while nobody used it.
    [[nodiscard]] bool restartsQuietly() const noexcept { return m_restartsQuietly; }
    // How many terminals would lose a running program if Kodosi restarted now.
    [[nodiscard]] int restartStops() const noexcept { return m_restartStops; }
    // The host says whether a restart now loses nothing while nobody uses Kodosi.
    void applyRestart(bool quiet, int stops);
    // Kodosi restarts by itself to install an update only while it updates automatically.
    void setUpdatesAutomatically(bool automatic);

    void beginInitialStart();
    void stop() noexcept;
    void setUpdateInstalled();
    void offerRestart(const QString& launched, const QString& running);

    Q_INVOKABLE void retry();
    Q_INVOKABLE void restartHost();
    Q_INVOKABLE void restart();

signals:
    void stateChanged();
    void hostForegroundChanged();
    void updateInstalledChanged();
    void restartStopsChanged();
    void restartOffered();

private:
    static constexpr qsizetype MaximumErrorTextLength = 320;
    static constexpr int StartDelayMilliseconds = 16;

    RuntimeBridge& m_runtime;
    StartOperation m_startOperation;
    StopOperation m_stopOperation;
    StopHostOperation m_stopHostOperation;
    std::optional<OutdatedHost> m_outdatedHost;
    QString m_errorText;
    State m_state = State::Starting;
    std::uint64_t m_attemptToken = 0;
    bool m_initialStartBegun = false;
    bool m_attemptInFlight = false;
    bool m_stopping = false;
    bool m_updateInstalled = false;
    bool m_restartRequested = false;
    bool m_restartQuiet = false;
    bool m_restartsQuietly = false;
    bool m_updatesAutomatically = true;
    int m_restartStops = 0;
    bool m_outdatedHostRestarted = false;

    void restartIfQuiet();
    std::uint64_t beginAttempt();
    void executeAttempt(std::uint64_t token);
    void completeAttempt(
        std::uint64_t token,
        RuntimeBridge::Result result);
    void handleRuntimeRunningChanged(bool running);
    void completeHostStop(std::uint64_t token, HostStop result);
    void setOutdatedHost(std::optional<OutdatedHost> host);
    void fail(QString message);
    [[nodiscard]] static QString describe(const OutdatedHost& host);

    [[nodiscard]] static QString boundedText(
        QString text,
        QString fallback);
};

}
