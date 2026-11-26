#include "presentation/DevicesModel.hpp"
#include "presentation/Workspace.hpp"

#include <algorithm>

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonObject>
#include <QMimeData>

namespace kodosi {
namespace {
    void sortByLabel(QVariantList& devices)
    {
        std::ranges::sort(devices, [](const QVariant& left, const QVariant& right) {
            return QString::localeAwareCompare(left.toMap().value(QStringLiteral("label")).toString(),
                       right.toMap().value(QStringLiteral("label")).toString())
                < 0;
        });
    }
}

DevicesModel::DevicesModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

void DevicesModel::revoke(const QString& deviceId)
{
    m_workspace.sendUntracked(
        QStringLiteral("devices.revoke"), { { QStringLiteral("deviceId"), deviceId } });
}

void DevicesModel::sleep(const QString& deviceId)
{
    m_workspace.sendUntracked(QStringLiteral("power.sleep"), { { QStringLiteral("computer"), deviceId } });
}

void DevicesModel::setKeepAwake(const QString& keepAwake, const QString& deviceId)
{
    if (!deviceId.isEmpty()) {
        m_workspace.sendUntracked(QStringLiteral("power.keepAwake"),
            { { QStringLiteral("keepAwake"), keepAwake }, { QStringLiteral("computer"), deviceId } });
        return;
    }
    m_keepAwake = keepAwake;
    emit changed();
    m_workspace.sendUntracked(
        QStringLiteral("power.keepAwake"), { { QStringLiteral("keepAwake"), keepAwake } });
}

void DevicesModel::wake(const QString& deviceId)
{
    m_workspace.sendUntracked(QStringLiteral("power.wake"), { { QStringLiteral("deviceId"), deviceId } });
}

void DevicesModel::testWake(const QString& deviceId)
{
    m_workspace.sendUntracked(QStringLiteral("power.test"), { { QStringLiteral("deviceId"), deviceId } });
}

void DevicesModel::approve(const QString& code)
{
    if (!m_checkRequest.isEmpty() && m_checkedLabel.isEmpty())
        return;
    m_checkedCode = code;
    m_checkedLabel.clear();
    m_checkRequest = m_workspace.sendRequest(
        QStringLiteral("devices.link.check"), { { QStringLiteral("code"), code } });
}

void DevicesModel::confirmApproval()
{
    if (m_checkedCode.isEmpty())
        return;
    m_workspace.sendUntracked(
        QStringLiteral("devices.link.approve"), { { QStringLiteral("code"), m_checkedCode } });
    dropApproval();
}

void DevicesModel::dropApproval()
{
    m_checkedCode.clear();
    m_checkedLabel.clear();
    m_checkRequest.clear();
    emit changed();
}

void DevicesModel::requestApproval()
{
    m_workspace.sendUntracked(QStringLiteral("devices.link.startSelf"));
}

void DevicesModel::cancelApproval()
{
    m_workspace.sendUntracked(QStringLiteral("devices.link.cancelSelf"));
}

void DevicesModel::offer()
{
    m_offerQr.clear();
    m_offerDetail.clear();
    m_offerState = m_workspace.sendUntracked(QStringLiteral("devices.link.offer"))
        ? QStringLiteral("requesting")
        : QString {};
    emit changed();
}

void DevicesModel::endOffer()
{
    if (m_offerState == QStringLiteral("requesting") || m_offerState == QStringLiteral("shown"))
        m_workspace.sendUntracked(QStringLiteral("devices.link.offer.cancel"));
    m_offerState.clear();
    m_offerQr.clear();
    m_offerDetail.clear();
    emit changed();
}

void DevicesModel::startFresh()
{
    m_workspace.sendUntracked(QStringLiteral("devices.reset"));
}

void DevicesModel::stopReset()
{
    m_workspace.sendUntracked(QStringLiteral("devices.reset.cancel"),
        { { QStringLiteral("resetId"), m_pendingReset.value(QStringLiteral("resetId")).toString() } });
}

QVariantMap DevicesModel::waitingReset() const
{
    const auto id = m_pendingReset.value(QStringLiteral("resetId")).toString();
    if (id.isEmpty() || id == m_dismissedReset || m_pendingReset.value(QStringLiteral("thisDevice")).toBool())
        return {};
    return m_pendingReset;
}

bool DevicesModel::hasRecoveryKey() const
{
    for (const auto& device : m_devices) {
        if (device.toMap().value(QStringLiteral("recoveryKey")).toBool())
            return true;
    }
    return false;
}

bool DevicesModel::hasOtherDevices() const
{
    return std::ranges::any_of(m_devices, [this](const QVariant& device) {
        const auto entry = device.toMap();
        return entry.value(QStringLiteral("deviceId")).toString() != m_selfDeviceId
            && !entry.value(QStringLiteral("recoveryKey")).toBool();
    });
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
    sortByLabel(computers);
    return computers;
}

QVariantList DevicesModel::asleep() const
{
    QVariantList asleep;
    for (const auto& device : m_devices) {
        const auto entry = device.toMap();
        const auto power = m_power.value(entry.value(QStringLiteral("deviceId")).toString()).toMap();
        if (power.value(QStringLiteral("canWake")).toBool()
            || power.value(QStringLiteral("busy")).toString() == QLatin1String("waking"))
            asleep.append(entry);
    }
    sortByLabel(asleep);
    return asleep;
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

QVariantList DevicesModel::waitingRequests() const
{
    QVariantList waiting;
    for (const auto& request : m_requests) {
        if (!m_dismissedRequests.contains(request.toMap().value(QStringLiteral("requestId")).toString()))
            waiting.append(request);
    }
    return waiting;
}

void DevicesModel::dismissRequests()
{
    m_dismissedReset = m_pendingReset.value(QStringLiteral("resetId")).toString();
    m_dismissedRequests.clear();
    for (const auto& request : m_requests)
        m_dismissedRequests.append(request.toMap().value(QStringLiteral("requestId")).toString());
    emit changed();
}

void DevicesModel::dismissApprovalNotice()
{
    m_approvalNoticeDismissed = true;
    emit changed();
}

void DevicesModel::reset()
{
    m_devices.clear();
    m_requests.clear();
    m_dismissedRequests.clear();
    m_pendingReset.clear();
    m_dismissedReset.clear();
    m_hosts.clear();
    m_power.clear();
    m_approvalCode.clear();
    m_approvalQr.clear();
    m_offerState.clear();
    m_offerQr.clear();
    m_offerDetail.clear();
    m_checkedCode.clear();
    m_checkedLabel.clear();
    m_checkRequest.clear();
    m_newRecoveryKey.clear();
    m_notice.clear();
    m_selfDeviceId.clear();
    m_enrolled = false;
    m_approvalNoticeDismissed = false;
    emit changed();
}
}
