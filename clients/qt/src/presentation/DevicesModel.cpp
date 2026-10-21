#include "presentation/DevicesModel.hpp"
#include "presentation/Workspace.hpp"

#include <algorithm>

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonObject>
#include <QMimeData>

namespace kodosi {
DevicesModel::DevicesModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

void DevicesModel::revoke(const QString& deviceId)
{
    m_workspace.sendUntracked(
        QStringLiteral("devices.revoke"), { { QStringLiteral("deviceId"), deviceId } });
}

void DevicesModel::approve(const QString& code)
{
    m_workspace.sendUntracked(
        QStringLiteral("devices.link.approve"), { { QStringLiteral("code"), code } });
}

void DevicesModel::requestApproval()
{
    m_workspace.sendUntracked(QStringLiteral("devices.link.startSelf"));
}

void DevicesModel::cancelApproval()
{
    m_workspace.sendUntracked(QStringLiteral("devices.link.cancelSelf"));
}

void DevicesModel::startFresh()
{
    m_workspace.sendUntracked(QStringLiteral("devices.reset"));
}

bool DevicesModel::hasRecoveryKey() const
{
    for (const auto& device : m_devices) {
        if (device.toMap().value(QStringLiteral("recoveryKey")).toBool())
            return true;
    }
    return false;
}

QVariantList DevicesModel::computers() const
{
    QVariantList computers;
    for (const auto& device : m_devices) {
        const auto entry = device.toMap();
        const auto id = entry.value(QStringLiteral("deviceId")).toString();
        if (m_hosts.contains(id) && id != m_selfDeviceId && !entry.value(QStringLiteral("recoveryKey")).toBool())
            computers.append(entry);
    }
    std::ranges::sort(computers, [](const QVariant& left, const QVariant& right) {
        return QString::localeAwareCompare(left.toMap().value(QStringLiteral("label")).toString(),
                   right.toMap().value(QStringLiteral("label")).toString())
            < 0;
    });
    return computers;
}

void DevicesModel::makeRecoveryKey()
{
    m_workspace.sendUntracked(QStringLiteral("devices.recovery.create"));
}

void DevicesModel::useRecoveryKey(const QString& key)
{
    m_workspace.sendUntracked(
        QStringLiteral("devices.recovery.use"), { { QStringLiteral("key"), key } });
}

void DevicesModel::copyNewRecoveryKey()
{
    auto* application = qobject_cast<QGuiApplication*>(QCoreApplication::instance());
    if (!application)
        return;
    auto* data = new QMimeData;
    data->setText(m_newRecoveryKey);
    data->setData(QStringLiteral("x-kde-passwordManagerHint"), QByteArrayLiteral("secret"));
    application->clipboard()->setMimeData(data, QClipboard::Clipboard);
}

void DevicesModel::forgetNewRecoveryKey()
{
    m_newRecoveryKey.clear();
    emit changed();
}

void DevicesModel::reset()
{
    m_devices.clear();
    m_requests.clear();
    m_hosts.clear();
    m_approvalCode.clear();
    m_newRecoveryKey.clear();
    m_notice.clear();
    m_selfDeviceId.clear();
    m_enrolled = false;
    emit changed();
}
}
