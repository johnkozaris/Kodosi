#pragma once

#include <QObject>
#include <QVariantList>

namespace kodosi {
class Workspace;

class DevicesModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList devices READ devices NOTIFY changed)
    Q_PROPERTY(QVariantList requests READ requests NOTIFY changed)
    Q_PROPERTY(QString approvalCode READ approvalCode NOTIFY changed)
    Q_PROPERTY(QString notice READ notice NOTIFY changed)
    Q_PROPERTY(bool localDeviceEnrolled READ localDeviceEnrolled NOTIFY changed)
    Q_PROPERTY(QString selfDeviceId READ selfDeviceId NOTIFY changed)
    Q_PROPERTY(QString newRecoveryKey READ newRecoveryKey NOTIFY changed)
    Q_PROPERTY(bool hasRecoveryKey READ hasRecoveryKey NOTIFY changed)

public:
    explicit DevicesModel(Workspace& workspace);

    QVariantList devices() const { return m_devices; }
    QVariantList requests() const { return m_requests; }
    QString approvalCode() const { return m_approvalCode; }
    QString notice() const { return m_notice; }
    bool localDeviceEnrolled() const { return m_enrolled; }
    QString selfDeviceId() const { return m_selfDeviceId; }
    QString newRecoveryKey() const { return m_newRecoveryKey; }
    bool hasRecoveryKey() const;

    Q_INVOKABLE void revoke(const QString& deviceId);
    Q_INVOKABLE void approve(const QString& code);
    Q_INVOKABLE void requestApproval();
    Q_INVOKABLE void cancelApproval();
    Q_INVOKABLE void startFresh();
    Q_INVOKABLE void makeRecoveryKey();
    Q_INVOKABLE void useRecoveryKey(const QString& key);
    Q_INVOKABLE void copyNewRecoveryKey();
    Q_INVOKABLE void forgetNewRecoveryKey();

signals:
    void changed();

private:
    friend class Workspace;

    Workspace& m_workspace;
    QVariantList m_devices;
    QVariantList m_requests;
    QString m_approvalCode;
    QString m_notice;
    QString m_selfDeviceId;
    QString m_newRecoveryKey;
    bool m_enrolled = false;

    void reset();
};
}
