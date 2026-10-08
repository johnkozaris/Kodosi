#pragma once

#include "platform/DesktopNotificationDriver.hpp"

#include <QDBusConnection>
#include <QDBusServiceWatcher>
#include <QHash>

namespace kodosi {

class FreedesktopNotificationDriver final : public DesktopNotificationDriver {
    Q_OBJECT

public:
    explicit FreedesktopNotificationDriver(QObject* parent = nullptr);

    void post(const DesktopNotification& notification) override;
    void withdraw(const QString& key) override;

private slots:
    void onActionInvoked(uint id, const QString& action);
    void onActivationToken(uint id, const QString& token);
    void onNotificationClosed(uint id, uint reason);

private:
    QDBusConnection m_bus;
    QDBusServiceWatcher m_serviceWatcher;
    QHash<QString, uint> m_idsByKey;
    QHash<uint, QString> m_keysById;
    QHash<uint, QString> m_activationTokens;
    QHash<QString, quint64> m_postGenerations;
    int m_pendingPosts = 0;
    quint64 m_nextPostGeneration = 0;
    quint64 m_serviceGeneration = 0;

    void close(uint id);
    void forget(uint id);
};

}
