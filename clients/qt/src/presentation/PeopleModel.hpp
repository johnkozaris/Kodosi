#pragma once

#include <QObject>
#include <QVariantList>

namespace kodosi {
class Workspace;

class PeopleModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList friends READ friends NOTIFY changed)
    Q_PROPERTY(QVariantList incoming READ incoming NOTIFY changed)
    Q_PROPERTY(QVariantList outgoing READ outgoing NOTIFY changed)
    Q_PROPERTY(QString invite READ invite NOTIFY changed)

public:
    explicit PeopleModel(Workspace& workspace);

    QVariantList friends() const { return m_friends; }
    QVariantList incoming() const { return m_incoming; }
    QVariantList outgoing() const { return m_outgoing; }
    QString invite() const { return m_invite; }

    Q_INVOKABLE void request(const QString& username);
    Q_INVOKABLE void accept(const QString& username);
    Q_INVOKABLE void decline(const QString& username);
    Q_INVOKABLE void cancel(const QString& username);
    Q_INVOKABLE void remove(const QString& username);
    Q_INVOKABLE void trust(const QString& username);
    Q_INVOKABLE void verify(const QString& username, const QString& invite);
    Q_INVOKABLE void copyInvite();
    Q_INVOKABLE QString displayName(const QString& userId) const;

signals:
    void changed();

private:
    friend class Workspace;

    Workspace& m_workspace;
    QVariantList m_friends;
    QVariantList m_incoming;
    QVariantList m_outgoing;
    QString m_invite;

    void reset();
    void receiveInvite(const QString& text);
};
}
