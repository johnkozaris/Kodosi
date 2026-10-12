#pragma once

#include "app/DeepLinkRouter.hpp"

#include <QJsonObject>
#include <QHash>
#include <QList>
#include <QObject>
#include <QSet>
#include <QStringList>

#include <optional>

namespace kodosi {
class DesktopSettings;
class DesktopStateModel;
class SessionCatalogModel;
class Workspace;

class SessionActionsModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    SessionActionsModel(Workspace& workspace, SessionCatalogModel& sessions,
        DesktopStateModel& desktop, DesktopSettings& settings);

    bool busy() const;

    Q_INVOKABLE void refresh();
    Q_INVOKABLE bool activate(const QString& sessionId);
    Q_INVOKABLE bool activateInRoom(const QString& sessionId);
    Q_INVOKABLE bool createInRoom(const QString& roomId, const QString& directory);
    Q_INVOKABLE bool minimize(const QString& sessionId);
    Q_INVOKABLE bool create(const QString& name, const QString& directory);
    Q_INVOKABLE bool createOnBranch(const QString& directory, const QString& branch);
    Q_INVOKABLE QString suggestedBranch() const;
    Q_INVOKABLE bool resume(const QString& provider, const QString& nativeConversationId,
        const QString& workingDirectory);
    Q_INVOKABLE bool close(const QString& sessionId);
    Q_INVOKABLE bool rename(const QString& sessionId, const QString& name);
    Q_INVOKABLE bool run(const QString& sessionId, const QString& command);
    Q_INVOKABLE bool takeCreated(const QString& sessionId);
    Q_INVOKABLE bool share(const QString& sessionId, const QStringList& userIds,
        const QStringList& expectedUserIds = {});
    Q_INVOKABLE bool leave(const QString& sessionId);
    Q_INVOKABLE bool attachMission(const QString& sessionId, const QString& missionId);

signals:
    void busyChanged();
    void activated(QString sessionId);

private:
    friend class Workspace;

    Workspace& m_workspace;
    SessionCatalogModel& m_sessions;
    DesktopStateModel& m_desktop;
    DesktopSettings& m_settings;
    std::optional<DeepLinkDestination> m_activeLink;
    QList<DeepLinkDestination> m_links;
    QHash<QString, QString> m_roomCreations;
    QSet<QString> m_created;
    void completeCreation(const QString& sessionId, const QString& roomId);
    bool activateSession(const QString& sessionId, bool inRoom);

    bool createInternal(const QString& name, const QString& directory,
        const QString& provider, const QString& nativeConversationId, const QString& roomId = {},
        const QString& branch = {});
    bool command(QString type, const QString& id, QJsonObject values = {}, bool ownerOnly = false);
    void route(const DeepLinkDestination& destination);
    void activatePendingLink();
    void reset(bool clearLinks);
};
}
