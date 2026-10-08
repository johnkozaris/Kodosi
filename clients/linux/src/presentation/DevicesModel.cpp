#include "presentation/DevicesModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonObject>

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

void DevicesModel::reset()
{
    m_devices.clear();
    m_requests.clear();
    m_approvalCode.clear();
    m_notice.clear();
    m_selfDeviceId.clear();
    m_enrolled = false;
    emit changed();
}
}
