#pragma once

#include <QObject>
#include <QHash>
#include <QJsonObject>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

namespace kodosi {
class SessionCatalogModel;
class Workspace;

class MissionsModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QVariantMap room READ room NOTIFY roomChanged)
    Q_PROPERTY(QVariantMap presentation READ presentation NOTIFY presentationChanged)
    Q_PROPERTY(QString issueRepository READ issueRepository NOTIFY roomChanged)
    Q_PROPERTY(QVariantList issues READ issues NOTIFY roomChanged)
    Q_PROPERTY(QVariantList missions READ missions NOTIFY catalogChanged)
    Q_PROPERTY(QVariantList invitations READ invitations NOTIFY catalogChanged)
    Q_PROPERTY(bool catalogTruncated READ catalogTruncated NOTIFY catalogChanged)
    Q_PROPERTY(QVariantMap selectedMission READ selectedMission NOTIFY selectionChanged)
    Q_PROPERTY(QVariantList members READ members NOTIFY selectionChanged)
    Q_PROPERTY(QStringList sessionIds READ sessionIds NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedMissionId READ selectedMissionId NOTIFY selectionChanged)

public:
    MissionsModel(Workspace& workspace, SessionCatalogModel& sessions);

    bool busy() const;
    QVariantMap room() const { return m_room; }
    QVariantMap presentation() const { return m_presentations.value(m_selectedId); }
    QString issueRepository() const { return m_issueRepository; }
    Q_INVOKABLE void setPresentation(const QString& key, const QVariant& value);
    QVariantList issues() const { return m_issues; }
    Q_INVOKABLE void roomAction(const QVariantMap& action);
    Q_INVOKABLE void copyAgentInstructions();
    QVariantList missions() const { return m_missions; }
    QVariantList invitations() const { return m_invitations; }
    bool catalogTruncated() const { return m_catalogTruncated; }
    QVariantMap selectedMission() const { return m_selected; }
    QVariantList members() const { return m_members; }
    QStringList sessionIds() const;
    QString selectedMissionId() const { return m_selectedId; }

    Q_INVOKABLE void create(const QString& name);
    Q_INVOKABLE void open(const QString& id);
    Q_INVOKABLE void rename(const QString& name);
    Q_INVOKABLE void remove();
    Q_INVOKABLE void invite(const QString& userId);
    Q_INVOKABLE void removeMember(const QString& userId);
    Q_INVOKABLE void leave();
    Q_INVOKABLE void acceptInvitation(const QString& id);
    Q_INVOKABLE void declineInvitation(const QString& id);

signals:
    void busyChanged();
    void roomChanged();
    void presentationChanged();
    void roomActionFinished(const QString& action);
    void created(const QString& roomId);
    void catalogChanged();
    void selectionChanged();

private:
    friend class Workspace;

    Workspace& m_workspace;
    SessionCatalogModel& m_sessions;
    QVariantList m_missions;
    QVariantList m_invitations;
    QVariantMap m_selected;
    QVariantList m_members;
    QString m_selectedId;
    QVariantMap m_room;
    QVariantList m_issues;
    QString m_issueRepository;
    QHash<QString, QVariantMap> m_presentations;
    QHash<QString, QVariantMap> m_rooms;
    void completeAction(const QString& room, const QString& action, const QVariantMap& submitted);
    void applyRoom(const QJsonObject& room);
    bool m_catalogTruncated = false;

    void command(QString type, QJsonObject values = {});
    void reset();
};
}
