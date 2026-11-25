#include "platform/linux/FreedesktopNotificationDriver.hpp"
#include "app/AppActivation.hpp"

#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QSet>
#include <QVariantMap>

#include <limits>

namespace kodosi {
namespace {

const auto service = QStringLiteral("org.freedesktop.Notifications");
const auto path = QStringLiteral("/org/freedesktop/Notifications");
const auto interface = QStringLiteral("org.freedesktop.Notifications");

QDBusMessage method(const QString& name)
{
    return QDBusMessage::createMethodCall(service, path, interface, name);
}

}

FreedesktopNotificationDriver::FreedesktopNotificationDriver(QObject* parent)
    : DesktopNotificationDriver(parent)
    , m_bus(QDBusConnection::sessionBus())
    , m_serviceWatcher(
          service,
          m_bus,
          QDBusServiceWatcher::WatchForOwnerChange,
          this)
{
    (void)m_bus.connect(
        service,
        path,
        interface,
        QStringLiteral("ActionInvoked"),
        this,
        SLOT(onActionInvoked(uint,QString)));
    (void)m_bus.connect(
        service,
        path,
        interface,
        QStringLiteral("ActivationToken"),
        this,
        SLOT(onActivationToken(uint,QString)));
    (void)m_bus.connect(
        service,
        path,
        interface,
        QStringLiteral("NotificationClosed"),
        this,
        SLOT(onNotificationClosed(uint,uint)));
    connect(
        &m_serviceWatcher,
        &QDBusServiceWatcher::serviceOwnerChanged,
        this,
        [this](
            const QString&,
            const QString& oldOwner,
            const QString& newOwner) {
            if (oldOwner.isEmpty() || oldOwner == newOwner) {
                return;
            }
            QSet<QString> invalidated;
            for (auto key = m_idsByKey.cbegin(); key != m_idsByKey.cend(); ++key) {
                invalidated.insert(key.key());
            }
            for (auto key = m_postGenerations.cbegin();
                 key != m_postGenerations.cend();
                 ++key) {
                invalidated.insert(key.key());
            }
            ++m_serviceGeneration;
            m_idsByKey.clear();
            m_keysById.clear();
            m_activationTokens.clear();
            m_postGenerations.clear();
            for (const auto& key : invalidated) {
                emit notificationClosed(key);
            }
        });
}

// Close open notifications on exit because a future instance cannot handle their clicks.
FreedesktopNotificationDriver::~FreedesktopNotificationDriver()
{
    QList<QDBusPendingCall> closing;
    for (auto id = m_keysById.cbegin(); id != m_keysById.cend(); ++id) {
        auto request = method(QStringLiteral("CloseNotification"));
        request.setArguments({QVariant::fromValue(id.key())});
        closing.append(m_bus.asyncCall(request, 1000));
    }
    for (auto& call : closing) {
        call.waitForFinished();
    }
}

void FreedesktopNotificationDriver::post(
    const DesktopNotification& notification)
{
    if (!notification.valid()) {
        emit notificationClosed(notification.key);
        return;
    }

    const auto replaced = m_idsByKey.value(notification.key);
    if (m_pendingPosts >= 32 || (replaced == 0 && m_idsByKey.size() >= 256)
        || m_nextPostGeneration == std::numeric_limits<quint64>::max()) {
        withdraw(notification.key);
        emit notificationClosed(notification.key);
        return;
    }
    const auto generation = ++m_nextPostGeneration;
    const auto serviceGeneration = m_serviceGeneration;
    m_postGenerations.insert(notification.key, generation);

    QVariantMap hints;
    hints.insert(
        QStringLiteral("desktop-entry"),
        QStringLiteral("com.kodosi.Kodosi"));
    // Low urgency suppresses popups on desktop shells, and GNOME only plays a sound when one is
    // explicitly specified.
    hints.insert(QStringLiteral("urgency"),
        QVariant::fromValue(static_cast<uchar>(notification.cue == NotificationCue::List ? 0 : 1)));
    if (notification.cue == NotificationCue::Sound)
        hints.insert(QStringLiteral("sound-name"), QStringLiteral("message-new-instant"));
    else
        hints.insert(QStringLiteral("suppress-sound"), true);

    auto request = method(QStringLiteral("Notify"));
    request.setArguments({
        QStringLiteral("Kodosi"),
        QVariant::fromValue(replaced),
        QStringLiteral("com.kodosi.Kodosi"),
        notification.title.toHtmlEscaped(),
        notification.body.toHtmlEscaped(),
        QStringList {QStringLiteral("default"), tr("Open Kodosi")},
        hints,
        -1,
    });
    auto* watcher = new QDBusPendingCallWatcher(m_bus.asyncCall(request, 5000), this);
    ++m_pendingPosts;
    connect(
        watcher,
        &QDBusPendingCallWatcher::finished,
        this,
        [this,
         key = notification.key,
         generation,
         serviceGeneration](QDBusPendingCallWatcher* call) {
            const QDBusPendingReply<uint> reply = *call;
            --m_pendingPosts;
            call->deleteLater();
            if (reply.isError()) {
                if (m_postGenerations.value(key) != generation
                    || m_serviceGeneration != serviceGeneration) {
                    return;
                }
                withdraw(key);
                emit notificationClosed(key);
                return;
            }

            const auto id = reply.value();
            if (id == 0 && m_postGenerations.value(key) == generation
                && m_serviceGeneration == serviceGeneration) {
                withdraw(key);
                emit notificationClosed(key);
                return;
            }
            if (m_postGenerations.value(key) != generation
                || m_serviceGeneration != serviceGeneration) {
                if (m_serviceGeneration == serviceGeneration && !m_keysById.contains(id)) {
                    close(id);
                }
                return;
            }
            m_postGenerations.remove(key);
            if (const auto prior = m_keysById.constFind(id);
                prior != m_keysById.cend() && prior.value() != key) {
                m_idsByKey.remove(prior.value());
            }
            if (const auto previous = m_idsByKey.value(key); previous != 0 && previous != id) {
                m_keysById.remove(previous);
                m_activationTokens.remove(previous);
                close(previous);
            }
            m_idsByKey.insert(key, id);
            m_keysById.insert(id, key);
        });
}

void FreedesktopNotificationDriver::withdraw(const QString& key)
{
    m_postGenerations.remove(key);
    const auto id = m_idsByKey.take(key);
    if (id == 0) {
        return;
    }
    m_keysById.remove(id);
    m_activationTokens.remove(id);
    close(id);
}

void FreedesktopNotificationDriver::onActionInvoked(
    const uint id,
    const QString& action)
{
    const auto key = m_keysById.value(id);
    if (key.isEmpty() || action != QStringLiteral("default")) {
        return;
    }
    emit actionInvoked(key, action, m_activationTokens.take(id));
}

void FreedesktopNotificationDriver::onActivationToken(
    const uint id,
    const QString& token)
{
    if (m_keysById.contains(id) && !token.isEmpty() && isSafeActivationToken(token)) {
        m_activationTokens.insert(id, token);
    }
}

void FreedesktopNotificationDriver::onNotificationClosed(
    const uint id,
    const uint reason)
{
    // Expired notifications can remain in the notification center, where clicks still invoke them
    // and withdraw() must still close them.
    constexpr uint expired = 1;
    if (reason == expired)
        return;
    const auto key = m_keysById.value(id);
    forget(id);
    if (!key.isEmpty()) {
        emit notificationClosed(key);
    }
}

void FreedesktopNotificationDriver::close(const uint id)
{
    auto request = method(QStringLiteral("CloseNotification"));
    request.setArguments({QVariant::fromValue(id)});
    (void)m_bus.call(request, QDBus::NoBlock);
}

void FreedesktopNotificationDriver::forget(const uint id)
{
    const auto key = m_keysById.take(id);
    if (!key.isEmpty() && m_idsByKey.value(key) == id) {
        m_idsByKey.remove(key);
    }
    m_activationTokens.remove(id);
}

}
