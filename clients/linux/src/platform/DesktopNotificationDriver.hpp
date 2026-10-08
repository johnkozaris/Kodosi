#pragma once

#include <QObject>
#include <QString>
#include <QStringList>

namespace kodosi {

struct DesktopNotification {
    QString key;
    QString title;
    QString body;
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
    void deliveryError(QString key, QString message);
};

}
