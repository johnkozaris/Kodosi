#pragma once

#include <QObject>
#include <QString>

#include <cstdint>
#include <optional>

namespace kodosi {
class Workspace;

class AccountModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool signedIn READ signedIn NOTIFY accountChanged)
    Q_PROPERTY(bool signingIn READ signingIn NOTIFY loginChanged)
    Q_PROPERTY(QString userId READ userId NOTIFY accountChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY accountChanged)
    Q_PROPERTY(QString userCode READ userCode NOTIFY loginChanged)
    Q_PROPERTY(QString verificationUri READ verificationUri NOTIFY loginChanged)
    Q_PROPERTY(bool deleting READ deleting NOTIFY deletionChanged)
    Q_PROPERTY(QString deletionUri READ deletionUri NOTIFY deletionChanged)

public:
    explicit AccountModel(Workspace& workspace);

    bool signedIn() const { return !m_userId.isEmpty(); }
    bool signingIn() const { return m_finalizing || !m_userCode.isEmpty(); }
    bool finalizing() const { return m_finalizing; }
    QString userId() const { return m_userId; }
    QString displayName() const { return m_displayName; }
    QString userCode() const { return m_userCode; }
    QString verificationUri() const { return m_verificationUri; }
    bool deleting() const { return m_deleting; }
    QString deletionUri() const { return m_deletionUri; }

    Q_INVOKABLE void login();
    Q_INVOKABLE void cancelLogin();
    Q_INVOKABLE void logout();
    Q_INVOKABLE void deleteAccount();
    Q_INVOKABLE void cancelDeletion();

signals:
    void accountChanged();
    void loginChanged();
    void deletionChanged();

private:
    friend class Workspace;

    Workspace& m_workspace;
    QString m_userId;
    QString m_displayName;
    QString m_userCode;
    QString m_verificationUri;
    QString m_deletionUri;
    std::optional<std::uint64_t> m_epoch;
    bool m_finalizing = true;
    bool m_deleting = false;

    void reset();
    void endDeletion();
};
}
