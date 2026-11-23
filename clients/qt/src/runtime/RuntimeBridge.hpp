#pragma once

#include "runtime/RuntimeEventFaults.hpp"

#include <QByteArray>
#include <QByteArrayView>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include <atomic>
#include <cstdint>
#include <expected>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>

namespace kodosi {

struct OutdatedHost {
    bool foreground = false;
    std::uint32_t pid = 0;
    std::uint32_t terminals = 0;
};

enum class HostStop { Stopped, Refused, Failed };

struct RuntimeFailure {
    std::int32_t ffiResult;
    QString message;
    std::optional<OutdatedHost> outdatedHost;
};

class CommandDispatcher {
public:
    using Result = std::expected<void, RuntimeFailure>;
    virtual ~CommandDispatcher() = default;
    [[nodiscard]] virtual Result send(QByteArrayView json) = 0;
};

// The events of a routed pipe skip the GUI thread: each one goes to its reader's thread in the
// order that the runtime sent it, with `afterDrop` when the app dropped events since the one
// before, until the pipe is unrouted.
class PipeRouter {
public:
    using Read = std::function<void(const QByteArray& event, bool afterDrop)>;
    virtual ~PipeRouter() = default;
    virtual void route(const QString& pipe, QObject& reader, Read read) = 0;
    virtual void unroute(const QString& pipe) = 0;
};

[[nodiscard]] inline QJsonObject lostAnswer(const QString& operation, const QString& requestId, const QString& message)
{
    return { { QStringLiteral("type"), operation.section(QLatin1Char('.'), 0, 0) + QStringLiteral(".error") },
        { QStringLiteral("operation"), operation }, { QStringLiteral("requestId"), requestId },
        { QStringLiteral("message"), message } };
}

struct TerminalSubscription {
    QString sessionId;
    QString subscriptionId;
    std::uint64_t generation;

    bool operator==(const TerminalSubscription&) const = default;
};
struct TerminalData {
    TerminalSubscription subscription;
    std::uint64_t sequence;
    QByteArray bytes;
};
struct TerminalControl {
    TerminalSubscription subscription;
    QByteArray json;
};
struct TerminalConnectResult {
    TerminalSubscription subscription;
    std::int32_t result;
};
struct TerminalCheckpoint {
    TerminalSubscription subscription;
    std::uint64_t nextSequence;
    std::uint16_t rows;
    std::uint16_t columns;
    QByteArray snapshot;
    bool console = false;
};

class TerminalCommandDispatcher : public CommandDispatcher {
public:
    [[nodiscard]] virtual bool isRunning() const noexcept = 0;
    [[nodiscard]] virtual Result connectTerminal(const TerminalSubscription& subscription) = 0;
    [[nodiscard]] virtual Result refreshTerminal(const TerminalSubscription& subscription) = 0;
    [[nodiscard]] virtual Result disconnectTerminal(const TerminalSubscription& subscription) = 0;
    [[nodiscard]] virtual Result sendTerminalInput(const TerminalSubscription& subscription,
        const QString& expectedRuntimeIncarnationId, QByteArrayView bytes)
        = 0;
};

class TerminalEventSink {
public:
    virtual ~TerminalEventSink() = default;
    virtual void receiveData(TerminalData data) noexcept = 0;
    virtual void receiveControl(TerminalControl control) noexcept = 0;
    virtual void receiveConnectResult(TerminalConnectResult result) noexcept = 0;

    [[nodiscard]] virtual bool installCheckpoint(TerminalCheckpoint checkpoint) noexcept = 0;
};

class RuntimeBridge : public QObject, public TerminalCommandDispatcher, public PipeRouter {
    Q_OBJECT
public:
    using Result = CommandDispatcher::Result;
    static constexpr std::uint32_t protocolVersion = 107;

    explicit RuntimeBridge(QObject* parent = nullptr);
    RuntimeBridge(TerminalEventSink& terminalSink, QObject* parent = nullptr);
    ~RuntimeBridge() override;
    RuntimeBridge(const RuntimeBridge&) = delete;
    RuntimeBridge& operator=(const RuntimeBridge&) = delete;

    [[nodiscard]] bool isRunning() const noexcept override;
    [[nodiscard]] bool hostForeground() const noexcept { return m_hostForeground; }
    void startAsync(std::function<void(Result)> completion);
    void stop() noexcept;
    void quit() noexcept;
    void stopHostAsync(std::function<void(HostStop)> completion);
    // Takes commands from any thread. A runtime callback holds a lock of the runtime while it
    // runs, and the runtime may call back on the sending thread, so no lock that a callback
    // takes may be held while a command goes.
    [[nodiscard]] Result send(QByteArrayView json) override;
    void route(const QString& pipe, QObject& reader, Read read) override;
    void unroute(const QString& pipe) override;
    [[nodiscard]] Result connectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result refreshTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result disconnectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result sendTerminalInput(const TerminalSubscription& subscription,
        const QString& expectedRuntimeIncarnationId, QByteArrayView bytes) override;

signals:
    void eventReceived(QJsonObject event, quint64 accountEpoch, QByteArray payload);
    void runningChanged(bool running);
    void hostForegroundChanged();
    // The events that the app dropped may hold the answers that its requests wait for, so each
    // request that waits ends with this message.
    void fellBehind(QString message);

private:
    struct CallbackContext {
        std::recursive_mutex mutex;
        RuntimeBridge* bridge = nullptr;
    };
    struct Backlog {
        int events = 0;
        qsizetype bytes = 0;
    };
    struct QueuedEvent {
        std::uint64_t dropGeneration = 0;
        QByteArray payload;
    };
    struct Route {
        QByteArray field;
        QObject* reader = nullptr;
        Read read;
        std::uint64_t dropGeneration = 0;
    };
    std::shared_ptr<CallbackContext> m_callbacks = std::make_shared<CallbackContext>();
    bool m_starting = false;
    bool m_catchUpAfterStart = false;
    bool m_hostForeground = false;
    QList<QueuedEvent> m_startupEvents;
    std::atomic<void*> m_handle { nullptr };
    TerminalEventSink* m_terminalSink = nullptr;
    std::atomic<std::uint64_t> m_activeGeneration { 0 };
    std::mutex m_queueMutex;
    std::uint64_t m_dropGeneration = 0;
    Backlog m_queued;
    Backlog m_dropped;
    std::uint64_t m_nextGeneration = 0;
    std::mutex m_accountMutex;
    QString m_accountUserId;
    std::uint64_t m_accountEpoch = 0;
    std::mutex m_routesMutex;
    QList<Route> m_routes;
    RuntimeEventFaults m_faults;
    [[nodiscard]] Result requireRunning() const;
    [[nodiscard]] Result terminalOperationResult(std::int32_t result, QString operation) const;
    [[nodiscard]] Result subscriptionCommand(const TerminalSubscription& subscription,
        std::int32_t (*command)(void*, const char*, const char*, std::uint64_t), QString operation) const;
    class Waiting;
    void queueEvent(const std::uint8_t* json, std::uintptr_t length);
    [[nodiscard]] bool routeEvent(std::uint64_t generation, const QueuedEvent& event);
    [[nodiscard]] bool claimed(const QByteArray& field, const QObject* reader);
    [[nodiscard]] bool dequeue(std::uint64_t generation, const QueuedEvent& event);
    void forgetQueue();
    void receiveQueued(std::uint64_t generation, const QueuedEvent& event);
    void catchUp(std::uint64_t generation);
    void deliverEvent(const QByteArray& payload);
    static RuntimeBridge* resolve(void* userdata) noexcept;
    static void event(const std::uint8_t* json, std::uintptr_t length, void* userdata) noexcept;
    static void terminalData(const char*, const char*, std::uint64_t, std::uint64_t, const std::uint8_t*,
        std::uintptr_t, void*) noexcept;
    static void terminalControl(
        const char*, const char*, std::uint64_t, const std::uint8_t*, std::uintptr_t, void*) noexcept;
    static void terminalConnectResult(const char*, const char*, std::uint64_t, std::int32_t, void*) noexcept;
    static std::int32_t terminalCheckpoint(const char*, const char*, std::uint64_t, std::uint64_t,
        std::uint16_t, std::uint16_t, bool, const std::uint8_t*, std::uintptr_t, void*) noexcept;
};

}
