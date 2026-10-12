#pragma once

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

namespace kodosi {

struct RuntimeFailure {
    enum class Code { ContractMismatch, StartRejected, RuntimeStopped, InvalidArgument, FfiRejected };
    Code code;
    std::int32_t ffiResult;
    QString message;
};

class CommandDispatcher {
public:
    using Result = std::expected<void, RuntimeFailure>;
    virtual ~CommandDispatcher() = default;
    [[nodiscard]] virtual Result send(QByteArrayView json) = 0;
};

struct TerminalSubscription {
    QString sessionId;
    QString subscriptionId;
    std::uint64_t generation;
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
struct TerminalSemanticCheckpoint {
    TerminalSubscription subscription;
    std::uint64_t nextSequence;
    std::uint16_t rows;
    std::uint16_t columns;
    QByteArray semanticJson;
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

    [[nodiscard]] virtual bool installSemanticCheckpoint(TerminalSemanticCheckpoint checkpoint) noexcept = 0;
};

class RuntimeBridge : public QObject, public TerminalCommandDispatcher {
    Q_OBJECT
public:
    using Result = CommandDispatcher::Result;
    explicit RuntimeBridge(QObject* parent = nullptr);
    RuntimeBridge(TerminalEventSink& terminalSink, QObject* parent = nullptr);
    ~RuntimeBridge() override;
    RuntimeBridge(const RuntimeBridge&) = delete;
    RuntimeBridge& operator=(const RuntimeBridge&) = delete;

    [[nodiscard]] bool isRunning() const noexcept override;
    [[nodiscard]] Result start();
    void startAsync(std::function<void(Result)> completion);
    void stop() noexcept;
    [[nodiscard]] Result send(QByteArrayView json) override;
    [[nodiscard]] Result connectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result refreshTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result disconnectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result sendTerminalInput(const TerminalSubscription& subscription,
        const QString& expectedRuntimeIncarnationId, QByteArrayView bytes) override;

signals:
    void eventReceived(QJsonObject event, quint64 accountEpoch, QByteArray payload);
    void runningChanged(bool running);
    void eventError(QString message);

private:
    struct CallbackContext {
        std::recursive_mutex mutex;
        RuntimeBridge* bridge = nullptr;
    };
    std::shared_ptr<CallbackContext> m_callbacks = std::make_shared<CallbackContext>();
    bool m_starting = false;
    QList<QByteArray> m_startupEvents;
    void* m_handle = nullptr;
    TerminalEventSink* m_terminalSink = nullptr;
    std::atomic<std::uint64_t> m_activeGeneration { 0 };
    std::atomic<qsizetype> m_queuedBytes { 0 };
    std::atomic<int> m_queuedEvents { 0 };
    std::atomic<bool> m_queueFailed { false };
    std::uint64_t m_nextGeneration = 0;
    QString m_accountUserId;
    std::uint64_t m_accountEpoch = 0;
    [[nodiscard]] Result requireRunning() const;
    [[nodiscard]] Result terminalOperationResult(std::int32_t result, QString operation) const;
    void queueEvent(const std::uint8_t* json, std::uintptr_t length);
    void deliverEvent(const QByteArray& payload);
    static RuntimeBridge* resolve(void* userdata) noexcept;
    static void event(const std::uint8_t* json, std::uintptr_t length, void* userdata) noexcept;
    static void terminalData(const char*, const char*, std::uint64_t, std::uint64_t, const std::uint8_t*,
        std::uintptr_t, void*) noexcept;
    static void terminalControl(
        const char*, const char*, std::uint64_t, const std::uint8_t*, std::uintptr_t, void*) noexcept;
    static void terminalConnectResult(const char*, const char*, std::uint64_t, std::int32_t, void*) noexcept;
    static std::int32_t terminalSemanticCheckpoint(const char*, const char*, std::uint64_t, std::uint64_t,
        std::uint16_t, std::uint16_t, const std::uint8_t*, std::uintptr_t, void*) noexcept;
};

}
