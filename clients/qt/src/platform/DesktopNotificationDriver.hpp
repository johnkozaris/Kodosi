#pragma once

#include "app/DeepLinkRouter.hpp"

#include <QObject>
#include <QString>
#include <QStringList>

namespace kodosi {

struct DesktopNotification {
    QString key;
    QString title;
    QString body;
    DeepLinkDestination destination;

    [[nodiscard]] bool valid() const noexcept
    {
        return !key.isEmpty() && key.size() <= 256 && !title.isEmpty() && title.size() <= 320
            && body.size() <= 480 && DeepLinkRouter::isSafeIdentity(destination.sessionId);
    }
};

class DesktopNotificationDriver : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;
    ~DesktopNotificationDriver() override = default;

    virtual void post(const DesktopNotification& notification) = 0;
    virtual void withdraw(const QString& key) = 0;

signals:
    void actionInvoked(
        QString key,
        QString action,
        QString activationToken);
    void notificationClosed(QString key);
};

}
