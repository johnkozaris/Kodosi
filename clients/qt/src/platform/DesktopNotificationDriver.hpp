#pragma once

#include "app/DeepLinkRouter.hpp"

#include <QObject>
#include <QString>
#include <QStringList>

namespace kodosi {

// Delivery level: banner with sound, silent banner, or notification center only.
enum class NotificationCue { Sound, Banner, List };

struct DesktopNotification {
    QString key;
    QString title;
    QString body;
    QString sessionId;
    QString computer;
    NotificationCue cue = NotificationCue::List;

    [[nodiscard]] bool valid() const noexcept
    {
        return !key.isEmpty() && key.size() <= 256 && !title.isEmpty() && title.size() <= 320
            && body.size() <= 480 && computer.size() <= 256 && DeepLinkRouter::sessionId(sessionId);
    }
};

class DesktopNotificationDriver : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;
    ~DesktopNotificationDriver() override = default;

    // Posting with the key of an active notification replaces it in place.
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
