#include "runtime/RuntimeBridge.hpp"
#include "runtime/JsonEnvelope.hpp"
#include <kodosi_runtime.h>

#include <QJsonDocument>
#include <QMetaObject>
#include <QThread>
#include <limits>
#include <utility>
#include <thread>

static_assert(KODOSI_FFI_ABI_VERSION == 7, "KodosiQT requires FFI ABI 7");

namespace kodosi {
namespace {
    constexpr qsizetype maximumEventBytes = 8 * 1024 * 1024;
    constexpr qsizetype maximumQueuedEventBytes = 16 * 1024 * 1024;
    QByteArray copyBytes(const std::uint8_t* bytes, std::uintptr_t length)
    {
        if (length == 0)
            return {};
        if (bytes == nullptr || length > static_cast<std::uintptr_t>(maximumEventBytes))
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
    RuntimeFailure failure(RuntimeFailure::Code code, std::int32_t result, QString message)
    {
        return { code, result, std::move(message) };
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
    return m_handle != nullptr;
}

RuntimeBridge::Result RuntimeBridge::start()
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (isRunning())
        return {};
    if (kodosi_protocol_version() != 51 || kodosi_abi_version() != 7) {
        return std::unexpected(failure(RuntimeFailure::Code::ContractMismatch, KODOSI_FFI_DESER_FAILED,
            tr("The linked runtime is incompatible with this version of Kodosi.")));
    }
    static const kodosi_callbacks_t callbacks {
        .on_event = &RuntimeBridge::event,
        .on_terminal_data = &RuntimeBridge::terminalData,
        .on_terminal_control = &RuntimeBridge::terminalControl,
        .on_terminal_connect_result = &RuntimeBridge::terminalConnectResult,
        .on_terminal_checkpoint = &RuntimeBridge::terminalSemanticCheckpoint,
    };
    m_accountEpoch = 0;
    m_queueFailed.store(false, std::memory_order_release);
    m_accountUserId.clear();
    m_activeGeneration.store(++m_nextGeneration, std::memory_order_release);
    m_handle = kodosi_start(&callbacks, sizeof(callbacks), m_callbacks.get());
    if (!m_handle) {
        m_activeGeneration.store(0, std::memory_order_release);
        return std::unexpected(failure(RuntimeFailure::Code::StartRejected, KODOSI_FFI_RUNTIME_STOPPED,
            tr("Kodosi could not start its terminal runtime.")));
    }
    emit runningChanged(true);
    return {};
}

void RuntimeBridge::startAsync(std::function<void(Result)> completion)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (isRunning()) { completion({}); return; }
    if (m_starting || kodosi_protocol_version() != 51 || kodosi_abi_version() != 7) {
        completion(std::unexpected(failure(RuntimeFailure::Code::StartRejected, KODOSI_FFI_RUNTIME_STOPPED,
            tr("The runtime is unavailable or still starting."))));
        return;
    }
    m_starting = true;
    m_accountEpoch = 0;
    m_accountUserId.clear();
    m_queueFailed.store(false, std::memory_order_release);
    const auto generation = ++m_nextGeneration;
    m_activeGeneration.store(generation, std::memory_order_release);
    std::thread([context = m_callbacks, generation, completion = std::move(completion)]() mutable {
        const kodosi_callbacks_t callbacks {
            &RuntimeBridge::event, &RuntimeBridge::terminalData, &RuntimeBridge::terminalControl,
            &RuntimeBridge::terminalConnectResult, &RuntimeBridge::terminalSemanticCheckpoint
        };
        auto* handle = kodosi_start(&callbacks, sizeof(callbacks), context.get());
        auto pending = std::shared_ptr<void*>(new void*(handle), [context](void** value) {
            auto* abandoned = *value;
            delete value;
            if (abandoned) std::thread([context, abandoned] { kodosi_stop(abandoned); }).detach();
        });
        {
            std::lock_guard lock(context->mutex);
            if (auto* bridge = context->bridge) {
                QMetaObject::invokeMethod(bridge, [context, generation, pending, completion = std::move(completion)]() mutable {
                    auto* current = context->bridge;
                    current->m_starting = false;
                    if (current->m_activeGeneration.load(std::memory_order_acquire) != generation) return;
                    auto* handle = std::exchange(*pending, nullptr);
                    current->m_handle = handle;
                    if (handle) {
                        emit current->runningChanged(true);
                        const auto events = std::exchange(current->m_startupEvents, {});
                        for (const auto& event : events) current->deliverEvent(event);
                        completion({});
                    } else {
                        current->m_activeGeneration.store(0, std::memory_order_release);
                        completion(std::unexpected(failure(RuntimeFailure::Code::StartRejected, KODOSI_FFI_RUNTIME_STOPPED,
                            tr("Kodosi could not start its terminal runtime."))));
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
    m_startupEvents.clear();
    m_activeGeneration.store(0, std::memory_order_release);
    if (!m_handle)
        return;
    auto* handle = std::exchange(m_handle, nullptr);
    kodosi_stop(handle);
    emit runningChanged(false);
}

RuntimeBridge::Result RuntimeBridge::send(QByteArrayView json)
{
    Q_ASSERT(thread() == QThread::currentThread());
    if (auto result = requireRunning(); !result)
        return result;
    if (json.isEmpty() || json.size() > maximumEventBytes) {
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument, KODOSI_FFI_PAYLOAD_TOO_LARGE,
            tr("The command exceeds the runtime limit.")));
    }
    const QByteArray command(json.data(), json.size());
    const auto fields = jsonObjectFields(command);
    if (!fields) {
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument,
            KODOSI_FFI_DESER_FAILED, tr("The command is invalid or contains duplicate fields.")));
    }
    QByteArray bytes = QByteArrayLiteral("{\"accountUserId\":");
    bytes += m_accountUserId.isEmpty() ? QByteArrayLiteral("null") : jsonStringBytes(m_accountUserId);
    bytes += QByteArrayLiteral(",\"accountEpoch\":") + QByteArray::number(m_accountEpoch);
    for (const auto& field : *fields) {
        if (field.name == QStringLiteral("accountUserId") || field.name == QStringLiteral("accountEpoch")) {
            return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument,
                KODOSI_FFI_DESER_FAILED, tr("The runtime owns the command account identity.")));
        }
        bytes += ',' + jsonStringBytes(field.name) + ':';
        bytes.append(field.value.data(), field.value.size());
    }
    bytes += '}';
    if (bytes.size() > maximumEventBytes)
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument,
            KODOSI_FFI_PAYLOAD_TOO_LARGE, tr("The command exceeds the runtime limit.")));
    return terminalOperationResult(
        kodosi_send_command(m_handle, reinterpret_cast<const std::uint8_t*>(bytes.constData()),
            static_cast<std::uintptr_t>(bytes.size())),
        tr("Command"));
}

RuntimeBridge::Result RuntimeBridge::connectTerminal(const TerminalSubscription& subscription)
{
    if (auto result = requireRunning(); !result)
        return result;
    const auto session = subscription.sessionId.toUtf8();
    const auto id = subscription.subscriptionId.toUtf8();
    if (!validIdentifier(session) || !validIdentifier(id)) {
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument, KODOSI_FFI_DESER_FAILED,
            tr("The terminal subscription is invalid.")));
    }
    return terminalOperationResult(
        kodosi_terminal_connect(m_handle, session.constData(), id.constData(), subscription.generation),
        tr("Terminal connection"));
}
RuntimeBridge::Result RuntimeBridge::refreshTerminal(const TerminalSubscription& subscription)
{
    if (auto result = requireRunning(); !result)
        return result;
    const auto session = subscription.sessionId.toUtf8();
    const auto id = subscription.subscriptionId.toUtf8();
    if (!validIdentifier(session) || !validIdentifier(id)) {
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument, KODOSI_FFI_DESER_FAILED,
            tr("The terminal subscription is invalid.")));
    }
    return terminalOperationResult(
        kodosi_terminal_refresh(m_handle, session.constData(), id.constData(), subscription.generation),
        tr("Terminal refresh"));
}
RuntimeBridge::Result RuntimeBridge::disconnectTerminal(const TerminalSubscription& subscription)
{
    if (auto result = requireRunning(); !result)
        return result;
    const auto session = subscription.sessionId.toUtf8();
    const auto id = subscription.subscriptionId.toUtf8();
    if (!validIdentifier(session) || !validIdentifier(id)) {
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument, KODOSI_FFI_DESER_FAILED,
            tr("The terminal subscription is invalid.")));
    }
    return terminalOperationResult(
        kodosi_terminal_disconnect(m_handle, session.constData(), id.constData(), subscription.generation),
        tr("Terminal disconnect"));
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
        return std::unexpected(failure(RuntimeFailure::Code::InvalidArgument, KODOSI_FFI_DESER_FAILED,
            tr("The terminal input target is invalid.")));
    }
    return terminalOperationResult(
        kodosi_terminal_input(m_handle, session.constData(), incarnation.constData(), id.constData(),
            subscription.generation, reinterpret_cast<const std::uint8_t*>(bytes.data()),
            static_cast<std::uintptr_t>(bytes.size())),
        tr("Terminal input"));
}
RuntimeBridge::Result RuntimeBridge::requireRunning() const
{
    if (isRunning())
        return {};
    return std::unexpected(failure(RuntimeFailure::Code::RuntimeStopped, KODOSI_FFI_RUNTIME_STOPPED,
        tr("The terminal runtime is not running.")));
}
RuntimeBridge::Result RuntimeBridge::terminalOperationResult(std::int32_t result, QString operation) const
{
    if (result == KODOSI_FFI_OK)
        return {};
    return std::unexpected(
        failure(RuntimeFailure::Code::FfiRejected, result, tr("%1 failed (%2).").arg(operation).arg(result)));
}

void RuntimeBridge::queueEvent(const std::uint8_t* json, std::uintptr_t length)
{
    const auto generation = m_activeGeneration.load(std::memory_order_acquire);
    if (!generation || length == 0 || length > static_cast<std::uintptr_t>(maximumEventBytes))
        return;
    const auto size = static_cast<qsizetype>(length);
    const auto queued = m_queuedBytes.fetch_add(size, std::memory_order_acq_rel);
    const auto queuedEvents = m_queuedEvents.fetch_add(1, std::memory_order_acq_rel);
    if (queued > maximumQueuedEventBytes - size || queuedEvents >= 4096) {
        m_queuedBytes.fetch_sub(size, std::memory_order_acq_rel);
        m_queuedEvents.fetch_sub(1, std::memory_order_acq_rel);
        if (m_queueFailed.exchange(true, std::memory_order_acq_rel))
            return;
        QMetaObject::invokeMethod(
            this,
            [this, generation] {
                if (m_activeGeneration.load(std::memory_order_acquire) != generation)
                    return;
                stop();
                emit eventError(
                    tr("Kodosi could not keep up with runtime updates. Restart the runtime to reconnect."));
            },
            Qt::QueuedConnection);
        return;
    }
    const auto payload = copyBytes(json, length);
    QMetaObject::invokeMethod(
        this,
        [this, generation, size, payload] {
            m_queuedBytes.fetch_sub(size, std::memory_order_acq_rel);
            m_queuedEvents.fetch_sub(1, std::memory_order_acq_rel);
            if (m_activeGeneration.load(std::memory_order_acquire) != generation)
                return;
            if (m_starting) {
                if (m_startupEvents.size() >= 4096) { stop(); return; }
                qsizetype bytes = payload.size();
                for (const auto& queued : m_startupEvents) bytes += queued.size();
                if (bytes > maximumQueuedEventBytes) { stop(); return; }
                m_startupEvents.append(payload);
                return;
            }
            deliverEvent(payload);
        },
        Qt::QueuedConnection);
}
void RuntimeBridge::deliverEvent(const QByteArray& payload)
{
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(payload, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        emit eventError(tr("The runtime returned an invalid event."));
        return;
    }
    const auto object = document.object();
    const auto type = object.value(QStringLiteral("type")).toString();
    const auto fields = jsonObjectFields(payload);
    std::optional<std::uint64_t> epoch;
    if (fields) {
        for (const auto& field : *fields) {
            if (field.name == QStringLiteral("accountEpoch")) epoch = jsonUnsigned(field.value);
        }
    }
    const auto user = object.value(QStringLiteral("accountUserId")).toString();
    if (!epoch || *epoch < m_accountEpoch)
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
    auto payload = copyBytes(bytes, length);
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
    auto payload = copyBytes(json, length);
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
std::int32_t RuntimeBridge::terminalSemanticCheckpoint(const char* session, const char* subscription,
    std::uint64_t generation, std::uint64_t nextSequence, std::uint16_t rows, std::uint16_t columns,
    const std::uint8_t* json, std::uintptr_t length, void* userdata) noexcept
{
    std::lock_guard lock(static_cast<CallbackContext*>(userdata)->mutex);
    auto* bridge = resolve(userdata);
    if (!bridge || !bridge->m_terminalSink || !bridge->m_activeGeneration.load(std::memory_order_acquire)) {
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    }
    auto payload = copyBytes(json, length);
    if (!length || payload.isEmpty())
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    return bridge->m_terminalSink->installSemanticCheckpoint(
               { copySubscription(session, subscription, generation), nextSequence, rows, columns,
                   std::move(payload) })
        ? KODOSI_FFI_OK
        : KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
}

}
