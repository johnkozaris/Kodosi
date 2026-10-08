#pragma once

#include "presentation/SessionCatalogModel.hpp"
#include "platform/DesktopNotificationDriver.hpp"
#include <QJsonObject>

#include <QHash>
#include <QObject>
#include <QQueue>

namespace kodosi {

class TerminalNotifications final : public QObject {
    Q_OBJECT

public:
    TerminalNotifications(
        SessionCatalogModel& sessions,
        DesktopNotificationDriver& driver,
        QObject* parent = nullptr);

    void apply(const QJsonObject& event);

signals:
    void sessionRequested(QString sessionId, QString activationToken);
    void deliveryError(QString message);

private:
    struct Context {
        QString sessionId;
        QString runtimeIncarnationId;
    };

    SessionCatalogModel& m_sessions;
    DesktopNotificationDriver& m_driver;
    QHash<QString, Context> m_contexts;
    QQueue<QString> m_order;

    void handleAction(
        const QString& key,
        const QString& action,
        const QString& activationToken);
    void remove(const QString& key, bool withdraw);
    void prune();
    [[nodiscard]] bool current(const Context& context) const;
    [[nodiscard]] static bool owns(const QString& key);
};

}
