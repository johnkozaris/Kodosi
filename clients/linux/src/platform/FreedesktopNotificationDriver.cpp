#include "platform/FreedesktopNotificationDriver.hpp"
#include "app/SingleInstanceGuard.hpp"

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

void FreedesktopNotificationDriver::post(
    const DesktopNotification& notification)
{
    if (notification.key.isEmpty() || notification.key.size() > 256
        || notification.title.isEmpty() || notification.title.size() > 320
        || notification.body.size() > 480) {
        emit deliveryError(
            notification.key,
            tr("Desktop notification data was invalid and was not sent."));
        emit notificationClosed(notification.key);
        return;
    }

    withdraw(notification.key);
    if (m_pendingPosts >= 32 || m_idsByKey.size() >= 256
        || m_nextPostGeneration == std::numeric_limits<quint64>::max()) {
        emit deliveryError(notification.key, tr("Desktop notification capacity is full."));
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
    hints.insert(
        QStringLiteral("urgency"),
        QVariant::fromValue(static_cast<uchar>(1)));

    auto request = method(QStringLiteral("Notify"));
    request.setArguments({
        QStringLiteral("Kodosi"),
        QVariant::fromValue(uint {0}),
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
                m_postGenerations.remove(key);
                emit deliveryError(
                    key,
                    tr("Desktop notification failed: %1")
                        .arg(reply.error().message()));
                emit notificationClosed(key);
                return;
            }

            const auto id = reply.value();
            if (id == 0 && m_postGenerations.value(key) == generation
                && m_serviceGeneration == serviceGeneration) {
                m_postGenerations.remove(key);
                emit deliveryError(
                    key,
                    tr("The desktop notification server returned an invalid identifier."));
                emit notificationClosed(key);
                return;
            }
            if (m_postGenerations.value(key) != generation
                || m_serviceGeneration != serviceGeneration) {
                if (m_serviceGeneration == serviceGeneration) {
                    close(id);
                }
                return;
            }
            m_postGenerations.remove(key);
            if (const auto prior = m_keysById.constFind(id);
                prior != m_keysById.cend() && prior.value() != key) {
                m_idsByKey.remove(prior.value());
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
    if (m_keysById.contains(id) && !token.isEmpty() && SingleInstanceGuard::isSafeActivationToken(token)) {
        m_activationTokens.insert(id, token);
    }
}

void FreedesktopNotificationDriver::onNotificationClosed(
    const uint id,
    const uint reason)
{
    Q_UNUSED(reason)
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
