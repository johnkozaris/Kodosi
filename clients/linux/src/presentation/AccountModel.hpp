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
    Q_PROPERTY(QString userCode READ userCode NOTIFY loginChanged)
    Q_PROPERTY(QString verificationUri READ verificationUri NOTIFY loginChanged)

public:
    explicit AccountModel(Workspace& workspace);

    bool signedIn() const { return !m_userId.isEmpty(); }
    bool signingIn() const { return m_finalizing || !m_userCode.isEmpty(); }
    bool finalizing() const { return m_finalizing; }
    QString userId() const { return m_userId; }
    QString userCode() const { return m_userCode; }
    QString verificationUri() const { return m_verificationUri; }

    Q_INVOKABLE void login();
    Q_INVOKABLE void cancelLogin();
    Q_INVOKABLE void logout();
    Q_INVOKABLE void deleteAccount();

signals:
    void accountChanged();
    void loginChanged();

private:
    friend class Workspace;

    Workspace& m_workspace;
    QString m_userId;
    QString m_userCode;
    QString m_verificationUri;
    std::optional<std::uint64_t> m_epoch;
    bool m_finalizing = true;

    void reset();
};
}
