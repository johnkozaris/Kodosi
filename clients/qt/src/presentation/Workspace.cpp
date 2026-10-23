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
    , m_dosi(*this, sessions)
{
    connect(&sessions, &SessionCatalogModel::visibleChanged, this, &Workspace::reportVisible);
}

bool Workspace::validName(const QString& name)
{
    return !name.trimmed().isEmpty() && name.trimmed().toUtf8().size() <= 128
        && !name.contains(QChar::Null);
}

QString Workspace::sendRequest(QString type, QJsonObject values)
{
    const auto id = requestId();
    values.insert(QStringLiteral("type"), type);
    values.insert(QStringLiteral("requestId"), id);
    const auto result = m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact));
    if (!result) {
        setError(result.error().message);
        return {};
    }
    return id;
}

bool Workspace::sendUntracked(QString type, QJsonObject values)
{
    return !sendRequest(std::move(type), std::move(values)).isEmpty();
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
    m_pending.insert(id,
        { domain, std::move(type), std::move(sessionId), std::move(missionId), m_missions.presentation() });
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

void Workspace::reportVisible()
{
    const auto visible = m_sessions.visible();
    if (!m_hostReady || m_reportedVisible == visible)
        return;
    if (sendUntracked(QStringLiteral("session.visible"),
            { { QStringLiteral("sessionIds"), QJsonArray::fromStringList(visible) } }))
        m_reportedVisible = visible;
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
    m_people.reset();
    m_devices.reset();
    m_missions.reset();
    m_dosi.reset();
    m_sessionActions.reset(clearLinks);
    m_desktop.clearSessions();
    m_desktop.clearLauncherDraft();
    m_sessions.resetRuntimeAuthority();
    notifyBusy();
}

void Workspace::reset()
{
    m_hostReady = false;
    m_reportedVisible.reset();
    clearAccountData(m_account.signedIn());
    m_account.reset();
}

void Workspace::route(const DeepLinkDestination& destination)
{
    m_sessionActions.route(destination);
}

void Workspace::receiveResult(const QString& type, const QJsonObject& event)
{
    if (type == QStringLiteral("room.result")) {
        if (const auto room = event.value(QStringLiteral("room")).toObject(); !room.isEmpty())
            m_missions.applyRoom(room);
        if (event.contains(QStringLiteral("issues"))
            && event.value(QStringLiteral("roomId")).toString() == m_missions.m_selectedId
            && event.value(QStringLiteral("repositoryId")).toString() == m_missions.m_issueRepository) {
            m_missions.m_issues = list(event, "issues");
            emit m_missions.roomChanged();
        }
    }
    const auto id = event.value(QStringLiteral("requestId")).toString();
    const auto found = m_pending.constFind(id);
    if (found == m_pending.cend() || event.value(QStringLiteral("operation")).toString() != found->operation)
        return;
    const auto pending = m_pending.take(id);
    notifyBusy();
    if (type == QStringLiteral("room.result"))
        m_missions.completeAction(pending.missionId, event.value(QStringLiteral("action")).toString(), pending.draft);
    const auto created = event.value(QStringLiteral("sessionId")).toString();
    if (pending.operation == QStringLiteral("session.create") && !created.isEmpty())
        m_sessionActions.completeCreation(created, pending.missionId);
    if (pending.operation == QStringLiteral("mission.open") && pending.missionId == m_missions.m_selectedId) {
        m_missions.m_selected = event.value(QStringLiteral("mission")).toObject().toVariantMap();
        m_missions.m_members = list(event, "members");
        emit m_missions.selectionChanged();
        m_missions.roomAction({ { QStringLiteral("type"), QStringLiteral("read") } });
    }
    if (pending.operation == QStringLiteral("mission.create")) {
        m_missions.open(id);
        emit m_missions.created(id);
    }
    if (pending.operation == QStringLiteral("mission.invitation.accept") && !pending.missionId.isEmpty())
        m_missions.open(pending.missionId);
    if ((pending.operation == QStringLiteral("mission.delete") || pending.operation == QStringLiteral("mission.leave"))
        && pending.missionId == m_missions.m_selectedId) {
        m_missions.open({});
    }
}

void Workspace::apply(const QJsonObject& event, std::uint64_t accountEpoch)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("system.updateRequired")) {
        setError(tr("This version of Kodosi is too old for the server. Update Kodosi."));
        return;
    }
    if (type == QStringLiteral("system.ready")) {
        m_hostReady = true;
        m_reportedVisible = QStringList {};
        reportVisible();
        emit hostReady();
        return;
    }
    if (type == QStringLiteral("auth.restoring")) {
        m_account.m_finalizing = false;
        m_account.m_restoring = event.value(QStringLiteral("reason")).toString();
        emit m_account.loginChanged();
        return;
    }
    if (type == QStringLiteral("auth.finalizing")) {
        const bool fromBrowser = !m_account.m_signInPage.isEmpty();
        m_account.m_finalizing = true;
        m_account.m_restoring.clear();
        m_account.m_signInPage.clear();
        emit m_account.loginChanged();
        if (fromBrowser)
            emit m_account.browserAnswered();
        return;
    }
    if (type == QStringLiteral("auth.ready") || type == QStringLiteral("auth.required")) {
        m_account.m_finalizing = false;
        m_account.m_restoring.clear();
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
        m_account.m_signInPage.clear();
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
    if (type == QStringLiteral("auth.browser")) {
        m_account.m_finalizing = false;
        m_account.m_restoring.clear();
        m_account.m_signInPage = event.value(QStringLiteral("url")).toString();
        emit m_account.loginChanged();
        emit m_account.signInPageReady(m_account.m_signInPage);
        return;
    }
    if (type.startsWith(QStringLiteral("dosi."))) {
        m_dosi.apply(type, event);
        return;
    }
    if (type == QStringLiteral("term.attention")) {
        m_sessions.alert(event.value(QStringLiteral("sessionId")).toString());
        return;
    }
    if (type == QStringLiteral("session.show")) {
        emit showRequested(event.value(QStringLiteral("sessionId")).toString());
        return;
    }
    if (type == QStringLiteral("sessions.snapshot")) {
        m_sessions.apply(event);
        if (!m_sessions.hasAuthoritativeSnapshot()) {
            setError(m_sessions.authorityError());
            return;
        }
        emit m_missions.selectionChanged();
        return;
    }
    if (type == QStringLiteral("friends.result")) {
        if (event.value(QStringLiteral("operation")).toString() == QStringLiteral("friends.invite"))
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
    if (type == QStringLiteral("devices.hosts")) {
        m_devices.m_hosts = event.value(QStringLiteral("deviceIds")).toVariant().toStringList();
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
    if (type == QStringLiteral("devices.result")) {
        const auto operation = event.value(QStringLiteral("operation")).toString();
        if (operation == QStringLiteral("devices.link.startSelf"))
            m_devices.m_approvalCode = event.value(QStringLiteral("code")).toString();
        else if (operation == QStringLiteral("devices.recovery.create"))
            m_devices.m_newRecoveryKey = event.value(QStringLiteral("key")).toString();
        else
            return;
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("room.snapshot")) {
        m_missions.applyRoom(event.value(QStringLiteral("room")).toObject());
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
    if (type == QStringLiteral("session.result") || type == QStringLiteral("room.result")
        || type == QStringLiteral("mission.result")) {
        receiveResult(type, event);
        return;
    }
    if (type == QStringLiteral("provider.error") || type == QStringLiteral("folders.error")
        || !type.endsWith(QStringLiteral(".error")))
        return;
    const auto id = event.value(QStringLiteral("requestId")).toString();
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
        m_account.m_signInPage.clear();
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
