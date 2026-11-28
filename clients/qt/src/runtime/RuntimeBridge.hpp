#pragma once

#include "runtime/RuntimeEventFaults.hpp"

#include <QByteArray>
#include <QByteArrayView>
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QRectF>
#include <QString>

#include <atomic>
#include <cstdint>
#include <expected>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>

struct kodosi_video_frame_t;

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

// The decoder of a video pipe that the app watches. The runtime calls it on a thread of the pipe,
// one call at a time and in pipe order, so it never waits for the GUI thread.
class VideoSink {
public:
    struct Frame {
        // KODOSI_VIDEO_H264 or KODOSI_VIDEO_HEVC.
        std::uint8_t codec = 0;
        bool key = false;
        // The part of the source that the frame shows, in the source's points, when only a part.
        std::optional<QRectF> region;
        // The Annex B picture, valid only during the call.
        QByteArrayView data;
    };
    virtual ~VideoSink() = default;
    // Whether the decoder took the frame: false only when it cannot go on, so the runtime waits for
    // a keyframe and asks for one.
    [[nodiscard]] virtual bool take(const Frame& frame) noexcept = 0;
    // A JSON message of the stream, valid only during the call.
    virtual void hand(QByteArrayView message) noexcept = 0;
    // True when the picture is not the stream's newest any more, so it rests until the next frame;
    // false when it is the newest again.
    virtual void stale(bool stale) noexcept = 0;
};

// Hands the frames and messages of the video pipes that the app opened to their sinks.
class VideoWatcher {
public:
    virtual ~VideoWatcher() = default;
    // From now on until `unwatch` or the pipe's close.
    virtual void watch(const QString& pipe, std::shared_ptr<VideoSink> sink) = 0;
    // Returns once no call of the pipe's sink runs and none will come.
    virtual void unwatch(const QString& pipe) = 0;
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

class RuntimeBridge : public QObject, public TerminalCommandDispatcher, public VideoWatcher {
    Q_OBJECT
public:
    using Result = CommandDispatcher::Result;
    static constexpr std::uint32_t protocolVersion = 120;

    explicit RuntimeBridge(QObject* parent = nullptr);
    RuntimeBridge(TerminalEventSink& terminalSink, QObject* parent = nullptr);
    ~RuntimeBridge() override;
    RuntimeBridge(const RuntimeBridge&) = delete;
    RuntimeBridge& operator=(const RuntimeBridge&) = delete;

    [[nodiscard]] bool isRunning() const noexcept override;
    [[nodiscard]] bool hostForeground() const noexcept { return m_hostForeground; }
    void startAsync(std::function<void(Result)> completion);
    void stop() noexcept;
    // `update` tells the host that it stops to install an update and comes right back.
    void quit(bool update = false) noexcept;
    void stopHostAsync(std::function<void(HostStop)> completion);
    // Thread-safe. Runtime callbacks hold an internal runtime lock and may fire synchronously on
    // the calling thread, so never hold any callback-acquired mutex across `send()`.
    [[nodiscard]] Result send(QByteArrayView json) override;
    void watch(const QString& pipe, std::shared_ptr<VideoSink> sink) override;
    void unwatch(const QString& pipe) override;
    [[nodiscard]] Result connectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result refreshTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result disconnectTerminal(const TerminalSubscription& subscription) override;
    [[nodiscard]] Result sendTerminalInput(const TerminalSubscription& subscription,
        const QString& expectedRuntimeIncarnationId, QByteArrayView bytes) override;

signals:
    void eventReceived(QJsonObject event, quint64 accountEpoch, QByteArray payload);
    void runningChanged(bool running);
    void hostForegroundChanged();
    // Emitted when event backlog overflow forces a resync; receivers should fail pending requests
    // with `message` because their replies may have been dropped.
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
    std::mutex m_videosMutex;
    QHash<QByteArray, std::shared_ptr<VideoSink>> m_videos;
    RuntimeEventFaults m_faults;
    [[nodiscard]] Result requireRunning() const;
    [[nodiscard]] Result terminalOperationResult(std::int32_t result, QString operation) const;
    [[nodiscard]] Result subscriptionCommand(const TerminalSubscription& subscription,
        std::int32_t (*command)(void*, const char*, const char*, std::uint64_t), QString operation) const;
    void queueEvent(const std::uint8_t* json, std::uintptr_t length);
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
    static std::shared_ptr<VideoSink> videoSink(const char* pipe, void* userdata) noexcept;
    static std::int32_t videoFrame(const char*, const kodosi_video_frame_t*, void*) noexcept;
    static void videoMessage(const char*, const std::uint8_t*, std::uintptr_t, void*) noexcept;
    static void videoStale(const char*, bool, void*) noexcept;
};

}
