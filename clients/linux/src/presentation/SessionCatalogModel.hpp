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
    Q_PROPERTY(QVariantList sessions READ sessions NOTIFY folderGroupsChanged)
    Q_PROPERTY(QStringList attention READ attention NOTIFY attentionChanged)
    Q_PROPERTY(bool working READ working NOTIFY folderGroupsChanged)
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
        QString programState;
        QString programKind;
        QString programTitle;
        QString programMessage;
        int programProgress = -1;
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
    [[nodiscard]] QVariantList sessions() const;
    [[nodiscard]] QStringList attention() const { return m_attention; }
    [[nodiscard]] bool working() const;
    Q_INVOKABLE void clearAttention(const QString& id);
    void noteAttention(const QString& id);
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
    void attentionChanged();
    void alertChanged(const QString& sessionId, const QString& text);

private:
    QVector<Session> m_sessions;
    QStringList m_attention;
    AuthorityState m_state = Loading;
    QString m_error;
    [[nodiscard]] static std::optional<Session> decode(const QJsonObject& object);
    [[nodiscard]] static PresentationSession presentation(const Session& session);
    [[nodiscard]] static bool isWorking(const Session& session);
    [[nodiscard]] static QString waitState(const Session& session);
    [[nodiscard]] static QString sign(const Session& session);
    [[nodiscard]] static QString signLabel(const Session& session);
    [[nodiscard]] static QString report(const Session& session);
    [[nodiscard]] static QString activity(const Session& session);
    [[nodiscard]] QVariantMap fields(const Session& session) const;
};

}
