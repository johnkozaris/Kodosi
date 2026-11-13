#pragma once

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QVariantList>

namespace kodosi {
class Workspace;

class PeopleModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList friends READ friends NOTIFY changed)
    Q_PROPERTY(QVariantList incoming READ incoming NOTIFY changed)
    Q_PROPERTY(QVariantList outgoing READ outgoing NOTIFY changed)
    Q_PROPERTY(QVariantList blocked READ blocked NOTIFY changed)
    Q_PROPERTY(bool reporting READ reporting NOTIFY reportingChanged)

public:
    explicit PeopleModel(Workspace& workspace);

    QVariantList friends() const { return m_friends; }
    QVariantList incoming() const { return m_incoming; }
    QVariantList outgoing() const { return m_outgoing; }
    QVariantList blocked() const { return m_blocked; }
    bool reporting() const { return !m_reportRequest.isEmpty(); }

    Q_INVOKABLE void request(const QString& username);
    Q_INVOKABLE void accept(const QString& username);
    Q_INVOKABLE void decline(const QString& username);
    Q_INVOKABLE void cancel(const QString& username);
    Q_INVOKABLE void remove(const QString& username);
    Q_INVOKABLE void block(const QString& username);
    Q_INVOKABLE void unblock(const QString& username);
    Q_INVOKABLE void report(const QString& username, const QString& note);
    Q_INVOKABLE QString clippedNote(const QString& note) const;
    Q_INVOKABLE void trust(const QString& username);
    Q_INVOKABLE void verify(const QString& username, const QString& invite);
    Q_INVOKABLE void copyInvite();
    Q_INVOKABLE QString displayName(const QString& userId) const;

signals:
    void changed();
    void inviteCopied();
    void reportingChanged();
    void reported();
    void reportFailed(const QString& message);

private:
    friend class Workspace;

    Workspace& m_workspace;
    QVariantList m_friends;
    QVariantList m_incoming;
    QVariantList m_outgoing;
    QVariantList m_blocked;
    QElapsedTimer m_inviteRequested;
    QString m_reportRequest;

    void reset();
    void receiveInvite(const QString& text);
    bool answerReport(const QJsonObject& answer);
};
}
