#pragma once

#include <QObject>
#include <QHash>
#include <QJsonObject>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

#include <functional>

namespace kodosi {
class SessionCatalogModel;
class Workspace;

class ProjectsModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QVariantMap content READ content NOTIFY contentChanged)
    Q_PROPERTY(QVariantMap presentation READ presentation NOTIFY presentationChanged)
    Q_PROPERTY(QString issueRepository READ issueRepository NOTIFY contentChanged)
    Q_PROPERTY(QVariantList issues READ issues NOTIFY contentChanged)
    Q_PROPERTY(QVariantMap issuesByRepository READ issuesByRepository NOTIFY contentChanged)
    Q_PROPERTY(QVariantMap repositoryChoices READ repositoryChoices NOTIFY repositoryChoicesChanged)
    Q_PROPERTY(QVariantList projects READ projects NOTIFY catalogChanged)
    Q_PROPERTY(QVariantList invitations READ invitations NOTIFY catalogChanged)
    Q_PROPERTY(bool catalogTruncated READ catalogTruncated NOTIFY catalogChanged)
    Q_PROPERTY(QVariantMap selectedProject READ selectedProject NOTIFY selectionChanged)
    Q_PROPERTY(QVariantList members READ members NOTIFY selectionChanged)
    Q_PROPERTY(QStringList sessionIds READ sessionIds NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedProjectId READ selectedProjectId NOTIFY selectionChanged)
    Q_PROPERTY(QStringList unreadProjects READ unreadProjects NOTIFY catalogChanged)

public:
    ProjectsModel(Workspace& workspace, SessionCatalogModel& sessions);

    std::function<bool()> isFront;
    void readWatched();
    bool busy() const;
    QVariantMap content() const { return m_content; }
    QVariantMap presentation() const { return m_presentations.value(m_selectedId); }
    QString issueRepository() const { return m_issueRepository; }
    Q_INVOKABLE void setPresentation(const QString& key, const QVariant& value);
    void followTerminal(const QString& replaced, const QString& id);
    QVariantList issues() const { return m_issues; }
    QVariantMap issuesByRepository() const { return m_issuesByRepository; }
    QVariantMap repositoryChoices() const { return m_repositoryChoices; }
    Q_INVOKABLE void listRepositories();
    Q_INVOKABLE void act(const QVariantMap& action);
    Q_INVOKABLE void copyAgentInstructions();
    QVariantList projects() const { return m_projects; }
    QVariantList invitations() const { return m_invitations; }
    bool catalogTruncated() const { return m_catalogTruncated; }
    QVariantMap selectedProject() const { return m_selected; }
    QVariantList members() const { return m_members; }
    QStringList sessionIds() const;
    QString selectedProjectId() const { return m_selectedId; }
    QStringList unreadProjects() const;

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
    void contentChanged();
    void presentationChanged();
    void created(const QString& projectId);
    void catalogChanged();
    void selectionChanged();
    void repositoryChoicesChanged();

private:
    friend class Workspace;

    Workspace& m_workspace;
    SessionCatalogModel& m_sessions;
    QVariantList m_projects;
    QVariantList m_invitations;
    QVariantMap m_selected;
    QVariantList m_members;
    QString m_selectedId;
    QVariantMap m_content;
    QVariantList m_issues;
    QString m_issueRepository;
    QVariantMap m_issuesByRepository;
    QVariantMap m_repositoryChoices;
    QHash<QString, QVariantMap> m_presentations;
    QHash<QString, QVariantMap> m_contents;
    QHash<QString, quint64> m_seen;
    void completeAction(const QString& projectId, const QString& action, const QVariantMap& submitted);
    void applyContent(const QJsonObject& content);
    bool watching(const QString& projectId) const;
    void see(const QString& projectId);
    void seeSection();
    bool m_catalogTruncated = false;

    void command(QString type, QJsonObject values = {});
    void reset();
};
}
