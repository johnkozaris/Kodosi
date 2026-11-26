#include "runtime/RuntimeBridge.hpp"
#include "runtime/JsonEnvelope.hpp"
#include <kodosi_runtime.h>

#include <QDebug>
#include <QJsonDocument>
#include <QMetaObject>
#include <QThread>
#include <QUuid>
#include <algorithm>
#include <utility>
#include <thread>

static_assert(KODOSI_FFI_ABI_VERSION == 11, "KodosiQT requires FFI ABI 11");

namespace kodosi {
namespace {
    constexpr qsizetype maximumEventBytes = 8 * 1024 * 1024;
    // Sized to hold a max-size (32 MiB) base64-encoded canvas transfer plus concurrent events.
    constexpr qsizetype maximumQueuedEventBytes = 64 * 1024 * 1024;
    constexpr int maximumQueuedEvents = 4096;
    QByteArray copyBytes(const std::uint8_t* bytes, std::uintptr_t length, std::uintptr_t maximum)
    {
        if (length == 0)
            return {};
        if (bytes == nullptr || length > maximum)
            return {};
        return QByteArray(reinterpret_cast<const char*>(bytes), static_cast<qsizetype>(length));
    }
    TerminalSubscription copySubscription(
        const char* sessionId, const char* subscriptionId, std::uint64_t generation)
    {
        return { sessionId ? QString::fromUtf8(sessionId) : QString {},
            subscriptionId ? QString::fromUtf8(subscriptionId) : QString {}, generation };
    }
    bool validIdentifier(const QByteArray& value)
    {
        return !value.isEmpty() && !value.contains('\0');
    }
    RuntimeFailure failure(std::int32_t result, QString message)
    {
        return { result, std::move(message), std::nullopt };
    }
    RuntimeFailure startFailure()
    {
        kodosi_start_failure_t raw {};
        if (kodosi_last_start_failure(&raw) != 1)
            return failure(KODOSI_FFI_RUNTIME_STOPPED, {});
        auto result = failure(KODOSI_FFI_RUNTIME_STOPPED, QString::fromUtf8(raw.message));
        if (raw.code == KODOSI_START_HOST_OUTDATED) {
            result.outdatedHost = OutdatedHost { .foreground = raw.host_kind == KODOSI_HOST_KIND_FOREGROUND,
                .pid = raw.host_pid,
                .terminals = raw.host_terminals };
        }
        return result;
    }

    // The runtime emits compact JSON and only pipe events carry a "pipe" key. Quotes inside JSON
    // strings are escaped, so substring matching `"pipe":"<id>"` safely identifies routed events.
    QByteArray pipeField(const QString& pipe)
    {
        return QByteArrayLiteral("\"pipe\":\"") + pipe.toUtf8() + '"';
    }
}

RuntimeBridge::RuntimeBridge(QObject* parent)
    : QObject(parent)
{
    m_callbacks->bridge = this;
}
RuntimeBridge::RuntimeBridge(TerminalEventSink& sink, QObject* parent)
    : QObject(parent)
    , m_terminalSink(&sink)
{
    m_callbacks->bridge = this;
}
RuntimeBridge::~RuntimeBridge()
{
    {
        std::lock_guard lock(m_callbacks->mutex);
        m_callbacks->bridge = nullptr;
    }
    stop();
}
bool RuntimeBridge::isRunning() const noexcept
{
    return m_handle.load(std::memory_order_acquire) != nullptr;
}

void RuntimeBridge::startAsync(std::function<void(Result)> completion)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (isRunning()) { completion({}); return; }
    if (kodosi_protocol_version() != protocolVersion || kodosi_abi_version() != KODOSI_FFI_ABI_VERSION) {
        completion(std::unexpected(failure(KODOSI_FFI_DESER_FAILED,
            tr("This copy of Kodosi is damaged. Install it again."))));
        return;
    }
    if (m_starting) {
        completion(std::unexpected(failure(KODOSI_FFI_RUNTIME_STOPPED,
            tr("Kodosi is still starting."))));
        return;
    }
    m_starting = true;
    {
        std::lock_guard lock(m_accountMutex);
        m_accountEpoch = 0;
        m_accountUserId.clear();
    }
    forgetQueue();
    const auto generation = ++m_nextGeneration;
    m_activeGeneration.store(generation, std::memory_order_release);
    std::thread([context = m_callbacks, generation, completion = std::move(completion)]() mutable {
        const kodosi_callbacks_t callbacks {
            &RuntimeBridge::event, &RuntimeBridge::terminalData, &RuntimeBridge::terminalControl,
            &RuntimeBridge::terminalConnectResult, &RuntimeBridge::terminalCheckpoint
        };
        auto* handle = kodosi_start(&callbacks, sizeof(callbacks), context.get());
        auto refusal = handle ? RuntimeFailure {} : startFailure();
        auto pending = std::shared_ptr<void*>(new void*(handle), [context](void** value) {
            auto* abandoned = *value;
            delete value;
            if (abandoned) std::thread([context, abandoned] { kodosi_stop(abandoned); }).detach();
        });
        {
            std::lock_guard lock(context->mutex);
            if (auto* bridge = context->bridge) {
                QMetaObject::invokeMethod(bridge, [context, generation, pending, refusal = std::move(refusal),
                                                      completion = std::move(completion)]() mutable {
                    auto* current = context->bridge;
                    current->m_starting = false;
                    if (current->m_activeGeneration.load(std::memory_order_acquire) != generation) return;
                    auto* handle = std::exchange(*pending, nullptr);
                    current->m_handle.store(handle, std::memory_order_release);
                    if (handle) {
                        emit current->runningChanged(true);
                        if (std::exchange(current->m_catchUpAfterStart, false))
                            current->catchUp(generation);
                        const auto events = std::exchange(current->m_startupEvents, {});
                        for (const auto& event : events) current->receiveQueued(generation, event);
                        completion({});
                    } else {
                        current->m_activeGeneration.store(0, std::memory_order_release);
                        if (refusal.message.isEmpty())
                            refusal.message = tr("Terminals could not start.");
                        completion(std::unexpected(std::move(refusal)));
                    }
                }, Qt::QueuedConnection);
                return;
            }
        }
    }).detach();
}

void RuntimeBridge::stop() noexcept
{
    Q_ASSERT(thread() == QThread::currentThread());
    m_activeGeneration.store(0, std::memory_order_release);
    forgetQueue();
    auto* handle = m_handle.exchange(nullptr, std::memory_order_acq_rel);
    if (!handle)
        return;
    kodosi_stop(handle);
    emit runningChanged(false);
}

void RuntimeBridge::quit(bool update) noexcept
{
    const bool stopsHost = isRunning() && !m_hostForeground;
    stop();
    if (stopsHost)
        kodosi_host_stop(1, update ? 1 : 0);
}

void RuntimeBridge::stopHostAsync(std::function<void(HostStop)> completion)
{
    Q_ASSERT(thread() == QThread::currentThread());
    std::thread([context = m_callbacks, completion = std::move(completion)]() mutable {
        const auto code = kodosi_host_stop(1, 0);
        const auto result = code == KODOSI_HOST_STOP_ACCEPTED || code == KODOSI_HOST_STOP_UNREACHABLE ? HostStop::Stopped
            : code == KODOSI_HOST_STOP_REFUSED                                                        ? HostStop::Refused
                                                                                                      : HostStop::Failed;
        std::lock_guard lock(context->mutex);
        if (auto* bridge = context->bridge) {
            QMetaObject::invokeMethod(
                bridge, [result, completion = std::move(completion)] { completion(result); }, Qt::QueuedConnection);
        }
    }).detach();
}

RuntimeBridge::Result RuntimeBridge::send(QByteArrayView json)
{
    auto* handle = m_handle.load(std::memory_order_acquire);
    if (!handle)
        return std::unexpected(failure(KODOSI_FFI_RUNTIME_STOPPED, tr("Kodosi is not ready.")));
    if (json.isEmpty() || json.size() > maximumEventBytes) {
        return std::unexpected(failure(KODOSI_FFI_PAYLOAD_TOO_LARGE,
            tr("The command is too large.")));
    }
    const QByteArray command(json.data(), json.size());
    const auto fields = jsonObjectFields(command);
    if (!fields) {
        return std::unexpected(failure(KODOSI_FFI_DESER_FAILED, tr("The command is invalid or contains duplicate fields.")));
    }
    QByteArray bytes = QByteArrayLiteral("{\"accountUserId\":");
    {
        std::lock_guard lock(m_accountMutex);
        bytes += m_accountUserId.isEmpty() ? QByteArrayLiteral("null") : jsonStringBytes(m_accountUserId);
        bytes += QByteArrayLiteral(",\"accountEpoch\":") + QByteArray::number(m_accountEpoch);
    }
    for (const auto& field : *fields) {
        if (field.name == QStringLiteral("accountUserId") || field.name == QStringLiteral("accountEpoch")) {
            return std::unexpected(failure(KODOSI_FFI_DESER_FAILED, tr("The command must not name the account.")));
        }
        bytes += ',' + jsonStringBytes(field.name) + ':';
        bytes.append(field.value.data(), field.value.size());
    }
    bytes += '}';
    if (bytes.size() > maximumEventBytes)
        return std::unexpected(failure(KODOSI_FFI_PAYLOAD_TOO_LARGE, tr("The command is too large.")));
    return terminalOperationResult(
        kodosi_send_command(handle, reinterpret_cast<const std::uint8_t*>(bytes.constData()),
            static_cast<std::uintptr_t>(bytes.size())),
        tr("Command"));
}

void RuntimeBridge::route(const QString& pipe, QObject& reader, Read read)
{
    std::uint64_t dropGeneration = 0;
    {
        std::lock_guard lock(m_queueMutex);
        dropGeneration = m_dropGeneration;
    }
    const auto field = pipeField(pipe);
    std::lock_guard lock(m_routesMutex);
    m_routes.removeIf([&field](const Route& route) { return route.field == field; });
    m_routes.append({ .field = field, .reader = &reader, .read = std::move(read), .dropGeneration = dropGeneration });
}

void RuntimeBridge::unroute(const QString& pipe)
{
    const auto field = pipeField(pipe);
    std::lock_guard lock(m_routesMutex);
    m_routes.removeIf([&field](const Route& route) { return route.field == field; });
}

RuntimeBridge::Result RuntimeBridge::connectTerminal(const TerminalSubscription& subscription)
{
    return subscriptionCommand(subscription, kodosi_terminal_connect, tr("Terminal connection"));
}
RuntimeBridge::Result RuntimeBridge::refreshTerminal(const TerminalSubscription& subscription)
{
    return subscriptionCommand(subscription, kodosi_terminal_refresh, tr("Terminal refresh"));
}
RuntimeBridge::Result RuntimeBridge::disconnectTerminal(const TerminalSubscription& subscription)
{
    return subscriptionCommand(subscription, kodosi_terminal_disconnect, tr("Terminal disconnect"));
}
RuntimeBridge::Result RuntimeBridge::subscriptionCommand(const TerminalSubscription& subscription,
    std::int32_t (*command)(void*, const char*, const char*, std::uint64_t), QString operation) const
{
    if (auto result = requireRunning(); !result)
        return result;
    const auto session = subscription.sessionId.toUtf8();
    const auto id = subscription.subscriptionId.toUtf8();
    if (!validIdentifier(session) || !validIdentifier(id)) {
        return std::unexpected(failure(KODOSI_FFI_DESER_FAILED,
            tr("The terminal subscription is invalid.")));
    }
    return terminalOperationResult(command(m_handle.load(std::memory_order_acquire), session.constData(),
                                       id.constData(), subscription.generation),
        std::move(operation));
}
RuntimeBridge::Result RuntimeBridge::sendTerminalInput(const TerminalSubscription& subscription,
    const QString& expectedRuntimeIncarnationId, QByteArrayView bytes)
{
    if (auto result = requireRunning(); !result)
        return result;
    const auto session = subscription.sessionId.toUtf8();
    const auto id = subscription.subscriptionId.toUtf8();
    const auto incarnation = expectedRuntimeIncarnationId.toUtf8();
    if (!validIdentifier(session) || !validIdentifier(id) || !validIdentifier(incarnation)
        || bytes.size() > maximumEventBytes) {
        return std::unexpected(failure(KODOSI_FFI_DESER_FAILED,
            tr("The terminal input target is invalid.")));
    }
    return terminalOperationResult(
        kodosi_terminal_input(m_handle.load(std::memory_order_acquire), session.constData(),
            incarnation.constData(), id.constData(), subscription.generation,
            reinterpret_cast<const std::uint8_t*>(bytes.data()),
            static_cast<std::uintptr_t>(bytes.size())),
        tr("Terminal input"));
}
RuntimeBridge::Result RuntimeBridge::requireRunning() const
{
    if (isRunning())
        return {};
    return std::unexpected(failure(KODOSI_FFI_RUNTIME_STOPPED,
        tr("Kodosi is not ready.")));
}
RuntimeBridge::Result RuntimeBridge::terminalOperationResult(std::int32_t result, QString operation) const
{
    if (result == KODOSI_FFI_OK)
        return {};
    return std::unexpected(
        failure(result, tr("%1 failed (%2).").arg(operation, QString::number(result))));
}

void RuntimeBridge::queueEvent(const std::uint8_t* json, std::uintptr_t length)
{
    const auto generation = m_activeGeneration.load(std::memory_order_acquire);
    if (!generation || !json || length == 0 || length > static_cast<std::uintptr_t>(maximumEventBytes))
        return;
    const auto size = static_cast<qsizetype>(length);
    std::unique_lock lock(m_queueMutex);
    if (m_queued.events >= maximumQueuedEvents || m_queued.bytes > maximumQueuedEventBytes - size) {
        const bool catchingUp = m_dropped.events > 0;
        m_dropped.events += m_queued.events + 1;
        m_dropped.bytes += m_queued.bytes + size;
        m_queued = {};
        ++m_dropGeneration;
        lock.unlock();
        if (!catchingUp)
            QMetaObject::invokeMethod(this, [this, generation] { catchUp(generation); }, Qt::QueuedConnection);
        return;
    }
    ++m_queued.events;
    m_queued.bytes += size;
    QueuedEvent event { .dropGeneration = m_dropGeneration, .payload = {} };
    lock.unlock();
    event.payload = copyBytes(json, length, static_cast<std::uintptr_t>(maximumEventBytes));
    if (routeEvent(generation, event))
        return;
    QMetaObject::invokeMethod(
        this, [this, generation, event = std::move(event)] { receiveQueued(generation, event); }, Qt::QueuedConnection);
}
// Tracks a routed event against the queue budget until the reader consumes it or is destroyed,
// allowing backpressure drops to discard queued routed events alongside GUI events.
class RuntimeBridge::Waiting {
public:
    Waiting(RuntimeBridge& bridge, std::uint64_t generation, QueuedEvent event)
        : m_bridge(bridge)
        , m_generation(generation)
        , m_event(std::move(event))
    {
    }
    ~Waiting()
    {
        if (!m_taken)
            (void)m_bridge.dequeue(m_generation, m_event);
    }
    Waiting(const Waiting&) = delete;
    Waiting& operator=(const Waiting&) = delete;
    [[nodiscard]] bool take()
    {
        m_taken = true;
        return m_bridge.dequeue(m_generation, m_event);
    }
    [[nodiscard]] const QByteArray& payload() const { return m_event.payload; }

private:
    RuntimeBridge& m_bridge;
    std::uint64_t m_generation;
    QueuedEvent m_event;
    bool m_taken = false;
};

bool RuntimeBridge::routeEvent(std::uint64_t generation, const QueuedEvent& event)
{
    std::lock_guard lock(m_routesMutex);
    const auto route = std::ranges::find_if(
        m_routes, [&event](const Route& route) { return event.payload.contains(route.field); });
    if (route == m_routes.end())
        return false;
    const bool afterDrop = std::exchange(route->dropGeneration, event.dropGeneration) != event.dropGeneration;
    QMetaObject::invokeMethod(
        route->reader,
        [this, waiting = std::make_shared<Waiting>(*this, generation, event), field = route->field,
            reader = route->reader, read = route->read, afterDrop] {
            if (waiting->take() && claimed(field, reader))
                read(waiting->payload(), afterDrop);
        },
        Qt::QueuedConnection);
    return true;
}
bool RuntimeBridge::claimed(const QByteArray& field, const QObject* reader)
{
    std::lock_guard lock(m_routesMutex);
    return std::ranges::any_of(
        m_routes, [&](const Route& route) { return route.field == field && route.reader == reader; });
}
bool RuntimeBridge::dequeue(std::uint64_t generation, const QueuedEvent& event)
{
    if (m_activeGeneration.load(std::memory_order_acquire) != generation)
        return false;
    std::lock_guard lock(m_queueMutex);
    if (event.dropGeneration != m_dropGeneration)
        return false;
    --m_queued.events;
    m_queued.bytes -= event.payload.size();
    return true;
}
void RuntimeBridge::forgetQueue()
{
    m_startupEvents.clear();
    m_catchUpAfterStart = false;
    std::lock_guard lock(m_queueMutex);
    ++m_dropGeneration;
    m_queued = {};
    m_dropped = {};
}
void RuntimeBridge::receiveQueued(std::uint64_t generation, const QueuedEvent& event)
{
    if (m_activeGeneration.load(std::memory_order_acquire) != generation)
        return;
    if (m_starting) {
        m_startupEvents.append(event);
        return;
    }
    if (dequeue(generation, event))
        deliverEvent(event.payload);
}
void RuntimeBridge::catchUp(std::uint64_t generation)
{
    if (m_activeGeneration.load(std::memory_order_acquire) != generation)
        return;
    if (m_starting) {
        m_startupEvents.clear();
        m_catchUpAfterStart = true;
        return;
    }
    Backlog dropped;
    {
        std::lock_guard lock(m_queueMutex);
        dropped = std::exchange(m_dropped, {});
    }
    qWarning().noquote() << QStringLiteral(
        "the app fell behind the runtime's events and dropped %1 of them, %2 bytes, so it asks for its state again")
                                .arg(dropped.events)
                                .arg(dropped.bytes);
    emit fellBehind(tr("Kodosi cannot tell if that worked. Check before you try again."));
    const QJsonObject resync { { QStringLiteral("type"), QStringLiteral("app.resync") },
        { QStringLiteral("requestId"), QUuid::createUuidV7().toString(QUuid::WithoutBraces) } };
    if (const auto sent = send(QJsonDocument(resync).toJson(QJsonDocument::Compact)); !sent)
        qWarning().noquote() << "the app could not ask for its state again:" << sent.error().message;
}
void RuntimeBridge::deliverEvent(const QByteArray& payload)
{
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(payload, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        m_faults.unreadable({}, error.error != QJsonParseError::NoError
                ? QStringLiteral("%1 at byte %2").arg(error.errorString()).arg(error.offset)
                : QStringLiteral("it is not an object"));
        return;
    }
    const auto object = document.object();
    const auto type = object.value(QStringLiteral("type")).toString();
    const auto fields = jsonObjectFields(payload, object);
    std::optional<std::uint64_t> epoch;
    if (fields) {
        for (const auto& field : *fields) {
            if (field.name == QStringLiteral("accountEpoch")) epoch = jsonUnsigned(field.value);
        }
    }
    if (!fields || !epoch) {
        m_faults.unreadable(type, fields ? QStringLiteral("its account epoch is missing or not a whole number")
                                    : QStringLiteral("it repeats a field or has too many"));
        return;
    }
    const auto user = object.value(QStringLiteral("accountUserId")).toString();
    {
        std::lock_guard lock(m_accountMutex);
        if (*epoch < m_accountEpoch)
            return;
        if (*epoch > m_accountEpoch && type != QStringLiteral("auth.ready")
            && type != QStringLiteral("auth.required") && type != QStringLiteral("system.ready"))
            return;
        if (*epoch == m_accountEpoch && user != m_accountUserId && type != QStringLiteral("auth.ready")
            && type != QStringLiteral("auth.required"))
            return;
        if (type == QStringLiteral("auth.ready") || type == QStringLiteral("auth.required")
            || type == QStringLiteral("system.ready")) {
            m_accountEpoch = *epoch;
            m_accountUserId = user;
        }
    }
    if (type == QStringLiteral("system.ready")) {
        const bool foreground = object.value(QStringLiteral("host")).toString() == QStringLiteral("foreground");
        if (std::exchange(m_hostForeground, foreground) != foreground)
            emit hostForegroundChanged();
    }
    emit eventReceived(object, *epoch, payload);
}
RuntimeBridge* RuntimeBridge::resolve(void* userdata) noexcept
{
    return static_cast<CallbackContext*>(userdata)->bridge;
}
void RuntimeBridge::event(const std::uint8_t* json, std::uintptr_t length, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    if (auto* bridge = resolve(userdata))
        bridge->queueEvent(json, length);
}
void RuntimeBridge::terminalData(const char* session, const char* subscription, std::uint64_t generation,
    std::uint64_t sequence, const std::uint8_t* bytes, std::uintptr_t length, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    auto* bridge = resolve(userdata);
    if (!bridge || !bridge->m_terminalSink || !bridge->m_activeGeneration.load(std::memory_order_acquire))
        return;
    auto payload = copyBytes(bytes, length, static_cast<std::uintptr_t>(maximumEventBytes));
    if (length && payload.isEmpty())
        return;
    bridge->m_terminalSink->receiveData(
        { copySubscription(session, subscription, generation), sequence, std::move(payload) });
}
void RuntimeBridge::terminalControl(const char* session, const char* subscription, std::uint64_t generation,
    const std::uint8_t* json, std::uintptr_t length, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    auto* bridge = resolve(userdata);
    if (!bridge || !bridge->m_terminalSink || !bridge->m_activeGeneration.load(std::memory_order_acquire))
        return;
    auto payload = copyBytes(json, length, KODOSI_TERMINAL_CONTROL_MAX_BYTES);
    if (length && payload.isEmpty())
        return;
    bridge->m_terminalSink->receiveControl(
        { copySubscription(session, subscription, generation), std::move(payload) });
}
void RuntimeBridge::terminalConnectResult(const char* session, const char* subscription,
    std::uint64_t generation, std::int32_t result, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    auto* bridge = resolve(userdata);
    if (!bridge || !bridge->m_terminalSink || !bridge->m_activeGeneration.load(std::memory_order_acquire))
        return;
    bridge->m_terminalSink->receiveConnectResult(
        { copySubscription(session, subscription, generation), result });
}
std::int32_t RuntimeBridge::terminalCheckpoint(const char* session, const char* subscription,
    std::uint64_t generation, std::uint64_t nextSequence, std::uint16_t rows, std::uint16_t columns,
    bool console, const std::uint8_t* snapshot, std::uintptr_t length, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    auto* bridge = resolve(userdata);
    if (!bridge || !bridge->m_terminalSink || !bridge->m_activeGeneration.load(std::memory_order_acquire)) {
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    }
    auto payload = copyBytes(snapshot, length, KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES);
    if (!length || payload.isEmpty())
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    return bridge->m_terminalSink->installCheckpoint(
               { copySubscription(session, subscription, generation), nextSequence, rows, columns,
                   std::move(payload), console })
        ? KODOSI_FFI_OK
        : KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
}

}
