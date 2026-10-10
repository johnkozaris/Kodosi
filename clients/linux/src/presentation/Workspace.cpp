#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QUuid>

#include <algorithm>
#include <ranges>

namespace kodosi {
namespace {
QVariantList list(const QJsonObject& object, const char* key)
{
    return object.value(QLatin1String(key)).toArray().toVariantList();
}

QString requestId()
{
    return QUuid::createUuidV7().toString(QUuid::WithoutBraces);
}
}

Workspace::Workspace(CommandDispatcher& commands, SessionCatalogModel& sessions,
    DesktopStateModel& desktop, DesktopSettings& settings, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_sessions(sessions)
    , m_desktop(desktop)
    , m_account(*this)
    , m_people(*this)
    , m_devices(*this)
    , m_missions(*this, sessions)
    , m_sessionActions(*this, sessions, desktop, settings)
{
    m_timeout.setInterval(1000);
    connect(&m_timeout, &QTimer::timeout, this, [this] {
        if (m_pending.isEmpty()) {
            m_timeout.stop();
            return;
        }
        bool expired = false;
        for (auto it = m_pending.begin(); it != m_pending.end();) {
            if (!it->deadline.hasExpired()) {
                ++it;
                continue;
            }
            if (it->operation == QStringLiteral("session.openRemote")
                && m_sessionActions.m_activeLink
                && m_sessionActions.m_activeLink->sessionId == it->sessionId) {
                m_sessionActions.m_activeLink.reset();
            }
            it = m_pending.erase(it);
            expired = true;
        }
        if (m_pending.isEmpty())
            m_timeout.stop();
        if (expired) {
            m_sessionActions.activatePendingLink();
            notifyBusy();
            setError(tr("The operation has not been confirmed. Refresh to check the current state."));
        }
    });
}

bool Workspace::validName(const QString& name)
{
    return !name.trimmed().isEmpty() && name.trimmed().toUtf8().size() <= 128
        && !name.contains(QChar::Null);
}

bool Workspace::sendUntracked(QString type, QJsonObject values, bool includeRequestId)
{
    values.insert(QStringLiteral("type"), type);
    if (includeRequestId)
        values.insert(QStringLiteral("requestId"), requestId());
    const auto result = m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact));
    if (!result) {
        setError(result.error().message);
        return false;
    }
    return true;
}

bool Workspace::sendTracked(PendingDomain domain, QString type, QJsonObject values,
    QString sessionId, QString missionId)
{
    if (m_pending.size() >= 32) {
        setError(tr("Wait for the current operations to finish."));
        return false;
    }
    const auto id = requestId();
    values.insert(QStringLiteral("type"), type);
    values.insert(QStringLiteral("requestId"), id);
    const auto result = m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact));
    if (!result) {
        setError(result.error().message);
        return false;
    }
    const auto timeout = type == QStringLiteral("room.command") ? 60000 : 15000;
    m_pending.insert(id,
        { domain, std::move(type), std::move(sessionId), std::move(missionId),
            QDeadlineTimer(timeout), m_missions.presentation() });
    if (!m_timeout.isActive())
        m_timeout.start();
    notifyBusy();
    clearError();
    return true;
}

bool Workspace::hasPending(PendingDomain domain) const
{
    return std::ranges::any_of(
        m_pending, [domain](const Pending& pending) { return pending.domain == domain; });
}

void Workspace::notifyBusy()
{
    emit m_sessionActions.busyChanged();
    emit m_missions.busyChanged();
}

void Workspace::clearError()
{
    if (m_error.isEmpty())
        return;
    m_error.clear();
    emit errorChanged();
}

void Workspace::setError(QString error)
{
    m_error = std::move(error);
    emit errorChanged();
}

void Workspace::clearAccountData(bool clearLinks)
{
    m_pending.clear();
    m_timeout.stop();
    m_people.reset();
    m_devices.reset();
    m_missions.reset();
    m_sessionActions.reset(clearLinks);
    m_desktop.clearSessions();
    m_sessions.resetRuntimeAuthority();
    notifyBusy();
}

void Workspace::reset()
{
    clearAccountData(m_account.signedIn());
    m_account.reset();
}

void Workspace::route(const DeepLinkDestination& destination)
{
    m_sessionActions.route(destination);
}

void Workspace::apply(const QJsonObject& event, std::uint64_t accountEpoch)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("auth.finalizing")) {
        m_account.m_finalizing = true;
        emit m_account.loginChanged();
        return;
    }
    if (type == QStringLiteral("auth.ready") || type == QStringLiteral("auth.required")) {
        m_account.m_finalizing = false;
        m_account.endDeletion();
        const auto user = type == QStringLiteral("auth.ready")
            ? event.value(QStringLiteral("userId")).toString()
            : QString {};
        if (accountEpoch != m_account.m_epoch || user != m_account.m_userId) {
            clearAccountData(m_account.signedIn());
            m_account.m_epoch = accountEpoch;
            m_account.m_userId = user;
            m_account.m_displayName.clear();
        }
        if (const auto name = event.value(QStringLiteral("displayName")).toString(); !name.isEmpty())
            m_account.m_displayName = name;
        m_account.m_userCode.clear();
        m_account.m_verificationUri.clear();
        m_devices.m_enrolled = !user.isEmpty() && event.value(QStringLiteral("enrolled")).toBool(true);
        if (m_devices.m_enrolled)
            m_devices.m_notice.clear();
        emit m_account.accountChanged();
        emit m_account.loginChanged();
        emit m_devices.changed();
        const auto reason = event.value(QStringLiteral("reason")).toString();
        if (reason == QStringLiteral("expired"))
            setError(tr("Your sign-in has ended. Sign in again."));
        else if (reason == QStringLiteral("accountDeleted"))
            setError(tr("Your Kodosi account is deleted."));
        m_sessionActions.refresh();
        m_sessionActions.activatePendingLink();
        return;
    }
    if (type == QStringLiteral("auth.deletion_pending")) {
        m_account.m_deleting = true;
        m_account.m_deletionUri = event.value(QStringLiteral("confirmationUri")).toString();
        emit m_account.deletionChanged();
        return;
    }
    if (type == QStringLiteral("auth.device_code")) {
        m_account.m_finalizing = false;
        m_account.m_userCode = event.value(QStringLiteral("userCode")).toString();
        m_account.m_verificationUri = event.value(QStringLiteral("verificationUri")).toString();
        emit m_account.loginChanged();
        return;
    }
    if (type == QStringLiteral("term.bell")) {
        m_sessions.noteAttention(event.value(QStringLiteral("sessionId")).toString());
        return;
    }
    if (type == QStringLiteral("sessions.snapshot")) {
        m_sessions.apply(event);
        if (!m_sessions.hasAuthoritativeSnapshot()) {
            setError(m_sessions.authorityError());
            return;
        }
        QHash<QString, QString> createdSessions;
        QList<QPair<QString, QString>> readyCreations;
        for (const auto& value : event.value(QStringLiteral("sessions")).toArray()) {
            const auto session = value.toObject();
            const auto request = session.value(QStringLiteral("createRequestId")).toString();
            if (!request.isEmpty())
                createdSessions.insert(request, session.value(QStringLiteral("id")).toString());
        }
        for (auto it = m_pending.begin(); it != m_pending.end();) {
            const auto pending = it.value();
            if (pending.operation == QStringLiteral("session.create")) {
                const auto created = createdSessions.value(it.key());
                if (!created.isEmpty()) {
                    it = m_pending.erase(it);
                    readyCreations.append({created, pending.missionId});
                    continue;
                }
            } else if (pending.operation == QStringLiteral("session.close")
                && !m_sessions.containsSession(pending.sessionId)) {
                it = m_pending.erase(it);
                continue;
            }
            ++it;
        }
        for (const auto& [created, room] : readyCreations) m_sessionActions.completeCreation(created, room);
        emit m_missions.selectionChanged();
        notifyBusy();
        return;
    }
    if (type == QStringLiteral("friends.invite")) {
        m_people.receiveInvite(event.value(QStringLiteral("text")).toString());
        return;
    }
    if (type == QStringLiteral("friends.snapshot")) {
        m_people.m_friends = list(event, "friends");
        m_people.m_incoming = list(event, "incoming");
        m_people.m_outgoing = list(event, "outgoing");
        emit m_people.changed();
        return;
    }
    if (type == QStringLiteral("devices.list")) {
        m_devices.m_devices = list(event, "devices");
        m_devices.m_selfDeviceId = event.value(QStringLiteral("selfDeviceId")).toString();
        m_devices.m_enrolled = event.value(QStringLiteral("localDeviceEnrolled")).toBool();
        m_devices.m_notice = m_devices.m_enrolled ? QString {} : event.value(QStringLiteral("notice")).toString();
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("devices.link.snapshot")) {
        m_devices.m_requests = list(event, "requests");
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("devices.link.selfPending")) {
        m_devices.m_approvalCode = event.value(QStringLiteral("code")).toString();
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("devices.link.selfResolved")) {
        m_devices.m_approvalCode.clear();
        emit m_devices.changed();
        sendUntracked(QStringLiteral("devices.refresh"));
        return;
    }
    if (type == QStringLiteral("devices.link.resolved")) {
        sendUntracked(QStringLiteral("devices.refresh"));
        return;
    }
    if (type == QStringLiteral("devices.recovery.created")) {
        m_devices.m_newRecoveryKey = event.value(QStringLiteral("key")).toString();
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("room.snapshot")) {
        m_missions.applyRoom(event.value(QStringLiteral("room")).toObject());
        return;
    }
    if (type == QStringLiteral("room.issues")) {
        if (event.value(QStringLiteral("roomId")).toString() == m_missions.m_selectedId
            && event.value(QStringLiteral("repositoryId")).toString() == m_missions.m_issueRepository) {
            m_missions.m_issues = list(event, "issues");
            emit m_missions.roomChanged();
        }
        return;
    }
    if (type == QStringLiteral("missions.snapshot")) {
        m_missions.m_missions = list(event, "missions");
        m_missions.m_invitations = list(event, "invitations");
        const bool missionsTruncated
            = event.value(QStringLiteral("missionsTruncated")).toBool();
        m_missions.m_catalogTruncated = missionsTruncated
            || event.value(QStringLiteral("invitationsTruncated")).toBool();
        emit m_missions.catalogChanged();
        if (!m_missions.m_selectedId.isEmpty()) {
            const auto exists = std::any_of(m_missions.m_missions.cbegin(),
                m_missions.m_missions.cend(), [&](const QVariant& mission) {
                    return mission.toMap().value(QStringLiteral("id")).toString()
                        == m_missions.m_selectedId;
                });
            if (!exists && !missionsTruncated)
                m_missions.open({});
            else
                m_missions.open(m_missions.m_selectedId);
        }
        return;
    }
    if (type == QStringLiteral("mission.snapshot")) {
        const auto mission = event.value(QStringLiteral("mission")).toObject();
        const auto id = event.value(QStringLiteral("requestId")).toString();
        const auto pending = m_pending.value(id);
        if (pending.operation != QStringLiteral("mission.open")
            || pending.missionId != m_missions.m_selectedId) {
            return;
        }
        if (mission.value(QStringLiteral("id")).toString() == m_missions.m_selectedId) {
            m_missions.m_selected = mission.toVariantMap();
            m_missions.m_members = list(event, "members");
            emit m_missions.selectionChanged();
            m_missions.roomAction({ { QStringLiteral("type"), QStringLiteral("read") } });
        }
    }
    const auto id = event.value(QStringLiteral("requestId")).toString();
    if (type == QStringLiteral("session.result")
        || type == QStringLiteral("room.result")
        || type == QStringLiteral("mission.result")
        || type == QStringLiteral("mission.snapshot")) {
        if (!m_pending.contains(id))
            return;
        const auto expected = m_pending.value(id).operation;
        if (type != QStringLiteral("mission.snapshot")
            && event.value(QStringLiteral("operation")).toString() != expected) {
            return;
        }
        const auto pending = m_pending.take(id);
        notifyBusy();
        if (type == QStringLiteral("room.result"))
            m_missions.completeAction(pending.missionId, event.value(QStringLiteral("action")).toString(), pending.draft);
        if (type == QStringLiteral("room.result") && event.value(QStringLiteral("roomId")).toString() == m_missions.m_selectedId)
            emit m_missions.roomActionFinished(event.value(QStringLiteral("action")).toString());
        if (pending.operation == QStringLiteral("session.create")
            && !event.value(QStringLiteral("sessionId")).toString().isEmpty()) {
            const auto sessionId = event.value(QStringLiteral("sessionId")).toString();
            m_sessionActions.completeCreation(sessionId, pending.missionId);
        }
        if (type == QStringLiteral("mission.result")) {
            if (pending.operation == QStringLiteral("mission.create")) { m_missions.open(id); emit m_missions.created(id); }
            if (pending.operation == QStringLiteral("mission.invitation.accept") && !pending.missionId.isEmpty()) m_missions.open(pending.missionId);
            if ((pending.operation == QStringLiteral("mission.delete")
                    || pending.operation == QStringLiteral("mission.leave"))
                && pending.missionId == m_missions.m_selectedId) {
                m_missions.open({});
            }
            sendUntracked(QStringLiteral("mission.list"));
        }
        return;
    }
    if (type == QStringLiteral("provider.error"))
        return;
    if (!type.endsWith(QStringLiteral(".error")))
        return;

    if (type == QStringLiteral("room.error")) {
        const auto pending = m_pending.take(id);
        m_missions.m_presentations[pending.missionId].insert(QStringLiteral("failure"), event.value(QStringLiteral("message")).toString());
        emit m_missions.presentationChanged();
        notifyBusy();
        return;
    }

    const auto pendingIt = m_pending.constFind(id);
    if (pendingIt != m_pending.cend()) {
        const auto pending = pendingIt.value();
        if (pending.operation == QStringLiteral("session.openRemote")
            && m_sessionActions.m_activeLink
            && m_sessionActions.m_activeLink->sessionId == pending.sessionId) {
            m_sessionActions.m_activeLink.reset();
            m_sessionActions.activatePendingLink();
        }
        if (pending.operation == QStringLiteral("mission.open")
            && pending.missionId == m_missions.m_selectedId) {
            m_missions.open({});
        }
        m_pending.remove(id);
        notifyBusy();
    }
    if (type == QStringLiteral("auth.error")) {
        m_account.m_finalizing = false;
        m_account.m_userCode.clear();
        emit m_account.loginChanged();
        if (event.value(QStringLiteral("operation")).toString() == QStringLiteral("auth.deleteAccount"))
            m_account.endDeletion();
    }
    setError(event.value(QStringLiteral("message")).toString());
    if (type == QStringLiteral("session.error")
        && event.value(QStringLiteral("operation")).toString()
            == QStringLiteral("session.list")) {
        m_sessions.fail(m_error);
    }
}
}
