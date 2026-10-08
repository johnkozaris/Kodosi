#pragma once

#include <QAbstractListModel>
#include <QJsonObject>
#include <QStringList>
#include <QVariantMap>
#include <optional>

namespace kodosi {

class SessionCatalogModel final : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(QVariantList folderGroups READ folderGroups NOTIFY folderGroupsChanged)
    Q_PROPERTY(AuthorityState authorityState READ authorityState NOTIFY authorityStateChanged)
    Q_PROPERTY(QString authorityError READ authorityError NOTIFY authorityStateChanged)
public:
    enum AuthorityState { Loading, Loaded, Failed };
    Q_ENUM(AuthorityState)
    struct Session {
        QString id;
        QString incarnationId;
        QString name;
        QString title;
        QString program;
        QStringList connectedUsers;
        QString kind;
        QString workingDirectory;
        QString ownerUserId;
        QString ownerName;
        QString hostName;
        QString missionId;
        QString missionName;
        QString status;
        QString connectionState;
        QString message;
        QString createRequestId;
        QStringList sharedWith;
        bool isOwner = false;
    };
    struct PresentationSession {
        QString id;
        QString kind;
        bool canRetainPresentation = false;
        bool isRemoteConnectable = false;
        bool canControl = false;
    };
    enum Role {
        SessionIdRole = Qt::UserRole + 1,
        NameRole,
        KindRole,
        HostRole,
        StatusRole,
        MissionIdRole,
        MissionNameRole,
        IsOwnerRole,
        ConnectionRole
    };
    explicit SessionCatalogModel(QObject* parent = nullptr);
    [[nodiscard]] QVariantList folderGroups() const;
    [[nodiscard]] int rowCount(const QModelIndex& parent = {}) const override;
    [[nodiscard]] QVariant data(const QModelIndex&, int role) const override;
    [[nodiscard]] QHash<int, QByteArray> roleNames() const override;
    [[nodiscard]] AuthorityState authorityState() const { return m_state; }
    [[nodiscard]] QString authorityError() const { return m_error; }
    [[nodiscard]] bool hasAuthoritativeSnapshot() const { return m_state == Loaded; }
    Q_INVOKABLE [[nodiscard]] bool containsSession(const QString& id) const;
    Q_INVOKABLE [[nodiscard]] QVariantMap presentationForSession(const QString& id) const;
    [[nodiscard]] std::optional<Session> session(const QString& id) const;
    [[nodiscard]] std::optional<PresentationSession> presentationSession(const QString& id) const;
    [[nodiscard]] std::optional<QString> incarnationForSession(const QString& id) const;
    void apply(const QJsonObject& event);
    void resetRuntimeAuthority();
    void beginRefresh();
    void fail(QString error);
signals:
    void folderGroupsChanged();
    void countChanged();
    void authorityStateChanged();
    void authoritativeSnapshotApplied();

private:
    QVector<Session> m_sessions;
    AuthorityState m_state = Loading;
    QString m_error;
    [[nodiscard]] static std::optional<Session> decode(const QJsonObject& object);
    [[nodiscard]] static PresentationSession presentation(const Session& session);
};

}
