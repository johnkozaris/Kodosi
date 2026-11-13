#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include "presentation/CanvasModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QStringList>
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

/// The failure of a refresh that the app asks for by itself is no news to the person: the runtime
/// refreshes again when the connection is back.
bool isRefresh(const QString& operation)
{
    static const QStringList refreshes { QStringLiteral("session.list"), QStringLiteral("auth.refresh"),
        QStringLiteral("friends.refresh"), QStringLiteral("devices.refresh"), QStringLiteral("project.list") };
    return refreshes.contains(operation);
}
}

Workspace::Workspace(CommandDispatcher& commands, SessionCatalogModel& sessions,
    DesktopStateModel& desktop, DesktopSettings& settings, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_sessions(sessions)
    , m_desktop(desktop)
    , m_settings(settings)
    , m_account(*this)
    , m_people(*this)
    , m_devices(*this)
    , m_projects(*this, sessions)
    , m_sessionActions(*this, sessions, desktop, settings)
    , m_dosi(*this, sessions)
{
    connect(&sessions, &SessionCatalogModel::visibleChanged, this, &Workspace::reportVisible);
    connect(&desktop, &DesktopStateModel::watchingChanged, this, &Workspace::reportVisible);
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

void Workspace::askRestore()
{
    if (!m_restoreAsked)
        m_restoreAsked = sendUntracked(QStringLiteral("session.restore"));
}

bool Workspace::sendTracked(PendingDomain domain, QString type, QJsonObject values,
    QString sessionId, QString projectId, bool quiet)
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
        { domain, std::move(type), std::move(sessionId), std::move(projectId), m_projects.presentation(), quiet });
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
    emit m_projects.busyChanged();
}

void Workspace::reportVisible()
{
    const auto visible = std::pair { m_sessions.visible(), m_desktop.watching() };
    if (!m_hostReady || m_reportedVisible == visible)
        return;
    if (sendUntracked(QStringLiteral("session.visible"),
            { { QStringLiteral("sessionIds"), QJsonArray::fromStringList(visible.first) },
                { QStringLiteral("watching"), visible.second } }))
        m_reportedVisible = visible;
}

void Workspace::attachCanvas(CanvasModel& canvas)
{
    m_canvas = &canvas;
    // Queued, so that the canvas opens after a change that moves the front, not in the middle of it.
    connect(&m_sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &Workspace::followCanvas,
        Qt::QueuedConnection);
    connect(&m_sessions, &SessionCatalogModel::visibleChanged, this, &Workspace::followCanvas, Qt::QueuedConnection);
    for (const auto signal : { &DesktopStateModel::activeViewChanged, &DesktopStateModel::stagedSessionIdsChanged,
             &DesktopStateModel::selectedSessionIdChanged, &DesktopStateModel::stageLayoutModeChanged })
        connect(&m_desktop, signal, this, &Workspace::followCanvas, Qt::QueuedConnection);
    connect(&m_projects, &ProjectsModel::selectionChanged, this, &Workspace::followCanvas, Qt::QueuedConnection);
    connect(&m_projects, &ProjectsModel::presentationChanged, this, &Workspace::followCanvas, Qt::QueuedConnection);
}

QString Workspace::frontSessionId() const
{
    const auto staged = m_desktop.stagedSessionIds();
    const auto selected = m_desktop.selectedSessionId();
    QString front;
    if (m_desktop.activeView() == DesktopStateModel::Sessions) {
        if (staged.contains(selected))
            front = selected;
        else if (m_desktop.stageLayoutMode() == DesktopStateModel::Grid && staged.size() == 1)
            front = staged.first();
    } else if (m_desktop.activeView() == DesktopStateModel::Projects) {
        const auto view = m_projects.presentation();
        const auto shown = view.value(QStringLiteral("terminal")).toString();
        if (view.value(QStringLiteral("section")).toInt() == 0 && staged.contains(shown)
            && m_projects.sessionIds().contains(shown))
            front = shown;
    }
    return m_sessions.visible().contains(front) ? front : QString {};
}

void Workspace::openCanvas(const QString& sessionId, const QString& section)
{
    if (m_desktop.activeView() == DesktopStateModel::Projects
        && m_projects.presentation().value(QStringLiteral("terminal")).toString() == sessionId) {
        m_canvas->open(sessionId, section);
        m_projects.setPresentation(QStringLiteral("showsCanvas"), true);
    } else {
        m_desktop.openCanvas(sessionId, section);
    }
}

bool Workspace::canvasOpen(const QString& sessionId) const
{
    if (!m_canvas->showsSomething(sessionId))
        return false;
    if (m_desktop.activeView() == DesktopStateModel::Projects) {
        const auto view = m_projects.presentation();
        return view.value(QStringLiteral("showsCanvas")).toBool() && view.value(QStringLiteral("terminal")).toString() == sessionId;
    }
    return m_desktop.canvasSessionId() == sessionId;
}

void Workspace::followCanvas()
{
    const auto id = frontSessionId();
    if (id.isEmpty() || !m_canvas)
        return;
    bool open = canvasOpen(id);
    for (const auto& news : m_canvas->news(id)) {
        const bool opens = news.shown ? open || m_settings.opensCanvas()
                                      : !open && m_settings.opensCanvas() && !m_canvas->isShut(id);
        if (!opens)
            continue;
        m_canvas->reveal(id, news.key);
        openCanvas(id, {});
        open = true;
    }
}

void Workspace::addFiles(const QString& sessionId, const QList<QUrl>& files)
{
    if (!m_canvas)
        return;
    bool sending = false;
    for (const auto& file : files)
        sending = m_canvas->attach(sessionId, file) || sending;
    if (sending)
        openCanvas(sessionId, QStringLiteral("files"));
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

void Workspace::clearAccountData(bool clearLinks, const QString& userId)
{
    m_pending.clear();
    m_account.setOnline(true);
    m_people.reset();
    m_devices.reset();
    m_projects.reset();
    m_dosi.reset();
    m_sessionActions.reset(clearLinks);
    m_desktop.switchAccount(userId);
    m_desktop.clearLauncherDraft();
    m_sessions.resetRuntimeAuthority();
    notifyBusy();
}

void Workspace::reset()
{
    m_hostReady = false;
    m_reportedVisible.reset();
    clearAccountData(m_account.signedIn(), m_account.m_userId);
    m_account.reset();
}

void Workspace::route(const DeepLinkDestination& destination)
{
    m_sessionActions.route(destination);
}

void Workspace::receiveResult(const QString& type, const QJsonObject& event)
{
    if (type == QStringLiteral("project.result")) {
        if (const auto content = event.value(QStringLiteral("content")).toObject(); !content.isEmpty())
            m_projects.applyContent(content);
        if (event.contains(QStringLiteral("issues"))
            && event.value(QStringLiteral("projectId")).toString() == m_projects.m_selectedId) {
            const auto repository = event.value(QStringLiteral("repositoryId")).toString();
            m_projects.m_issuesByRepository.insert(repository, list(event, "issues"));
            if (repository == m_projects.m_issueRepository)
                m_projects.m_issues = list(event, "issues");
            emit m_projects.contentChanged();
        }
    }
    const auto id = event.value(QStringLiteral("requestId")).toString();
    const auto found = m_pending.constFind(id);
    if (found == m_pending.cend() || event.value(QStringLiteral("operation")).toString() != found->operation)
        return;
    const auto pending = m_pending.take(id);
    notifyBusy();
    if (pending.operation == QStringLiteral("project.act"))
        m_projects.completeAction(pending.projectId, event.value(QStringLiteral("action")).toString(), pending.draft);
    const auto created = event.value(QStringLiteral("sessionId")).toString();
    if (pending.operation == QStringLiteral("session.create") && !created.isEmpty())
        m_sessionActions.completeCreation(created, pending.projectId);
    if (pending.operation == QStringLiteral("project.open") && pending.projectId == m_projects.m_selectedId) {
        m_projects.m_selected = event.value(QStringLiteral("project")).toObject().toVariantMap();
        m_projects.m_members = list(event, "members");
        emit m_projects.selectionChanged();
        m_projects.act({ { QStringLiteral("type"), QStringLiteral("read") } });
    }
    if (pending.operation == QStringLiteral("project.create")) {
        m_projects.open(id);
        emit m_projects.created(id);
    }
    if (pending.operation == QStringLiteral("project.invitation.accept") && !pending.projectId.isEmpty())
        m_projects.open(pending.projectId);
    if ((pending.operation == QStringLiteral("project.delete") || pending.operation == QStringLiteral("project.leave"))
        && pending.projectId == m_projects.m_selectedId) {
        m_projects.open({});
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
        m_restoreAsked = false;
        m_reportedVisible.reset();
        reportVisible();
        emit hostReady();
        return;
    }
    if (type == QStringLiteral("auth.restoring")) {
        if (accountEpoch != m_account.m_epoch) {
            clearAccountData(false, {});
            m_account.m_epoch = accountEpoch;
            m_account.m_userId.clear();
        }
        askRestore();
        m_account.m_finalizing = false;
        m_account.m_restoring = event.value(QStringLiteral("reason")).toString();
        m_account.receivePlace(event);
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
            clearAccountData(m_account.signedIn(), user);
            m_account.m_epoch = accountEpoch;
            m_account.m_userId = user;
            m_account.m_displayName.clear();
            if (!user.isEmpty())
                clearError();
        }
        if (const auto name = event.value(QStringLiteral("displayName")).toString(); !name.isEmpty())
            m_account.m_displayName = name;
        m_account.receivePlace(event);
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
        else if (reason == QStringLiteral("accountBanned"))
            setError(tr("This account is banned because it tried too many user names. Ask the operator of this Kodosi server to remove the ban."));
        askRestore();
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
    if (type == QStringLiteral("connection.state")) {
        m_account.setOnline(event.value(QStringLiteral("online")).toBool(true));
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
    if (type == QStringLiteral("session.picture")) {
        m_sessions.receivePicture(event);
        return;
    }
    if (type == QStringLiteral("session.show")) {
        emit showRequested(event.value(QStringLiteral("sessionId")).toString());
        return;
    }
    if (type == QStringLiteral("project.show")) {
        if (const auto id = event.value(QStringLiteral("projectId")).toString(); !id.isEmpty()) {
            m_desktop.bringToFront();
            m_desktop.setActiveView(DesktopStateModel::Projects);
            m_projects.open(id);
        }
        return;
    }
    if (type == QStringLiteral("sessions.snapshot")) {
        m_sessions.apply(event);
        if (!m_sessions.hasAuthoritativeSnapshot()) {
            setError(m_sessions.authorityError());
            return;
        }
        emit m_projects.selectionChanged();
        return;
    }
    if (type == QStringLiteral("friends.result")) {
        if (event.value(QStringLiteral("operation")).toString() == QStringLiteral("friends.invite"))
            m_people.receiveInvite(event.value(QStringLiteral("text")).toString());
        m_people.answerReport(event);
        return;
    }
    if (type == QStringLiteral("friends.snapshot")) {
        m_people.m_friends = list(event, "friends");
        m_people.m_incoming = list(event, "incoming");
        m_people.m_outgoing = list(event, "outgoing");
        m_people.m_blocked = list(event, "blocked");
        emit m_people.changed();
        return;
    }
    if (type == QStringLiteral("devices.list")) {
        m_devices.m_devices = list(event, "devices");
        m_devices.m_selfDeviceId = event.value(QStringLiteral("selfDeviceId")).toString();
        m_devices.m_enrolled = event.value(QStringLiteral("localDeviceEnrolled")).toBool();
        m_devices.m_notice = m_devices.m_enrolled ? QString {} : event.value(QStringLiteral("notice")).toString();
        m_devices.m_pendingReset = event.value(QStringLiteral("reset")).toObject().toVariantMap();
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
        else if (operation == QStringLiteral("devices.link.check") && !m_devices.m_checkRequest.isEmpty()
            && event.value(QStringLiteral("requestId")).toString() == m_devices.m_checkRequest)
            m_devices.m_checkedLabel = event.value(QStringLiteral("deviceLabel")).toString();
        else
            return;
        emit m_devices.changed();
        return;
    }
    if (type == QStringLiteral("project.snapshot")) {
        m_projects.applyContent(event.value(QStringLiteral("content")).toObject());
        return;
    }
    if (type == QStringLiteral("projects.snapshot")) {
        m_projects.m_projects = list(event, "projects");
        m_projects.m_invitations = list(event, "invitations");
        const bool projectsTruncated
            = event.value(QStringLiteral("projectsTruncated")).toBool();
        m_projects.m_catalogTruncated = projectsTruncated
            || event.value(QStringLiteral("invitationsTruncated")).toBool();
        emit m_projects.catalogChanged();
        if (!m_projects.m_selectedId.isEmpty()) {
            const auto exists = std::any_of(m_projects.m_projects.cbegin(),
                m_projects.m_projects.cend(), [&](const QVariant& project) {
                    return project.toMap().value(QStringLiteral("id")).toString()
                        == m_projects.m_selectedId;
                });
            if (!exists && !projectsTruncated)
                m_projects.open({});
            else
                m_projects.open(m_projects.m_selectedId);
        }
        return;
    }
    if (type == QStringLiteral("session.result") || type == QStringLiteral("project.result")) {
        receiveResult(type, event);
        return;
    }
    if (type == QStringLiteral("repositories.result") || type == QStringLiteral("repositories.error")) {
        m_projects.m_repositoryChoices = type == QStringLiteral("repositories.result")
            ? event.value(QStringLiteral("repositories")).toObject().toVariantMap()
            : QVariantMap { { QStringLiteral("failure"), event.value(QStringLiteral("message")).toString() } };
        emit m_projects.repositoryChoicesChanged();
        return;
    }
    if (type == QStringLiteral("provider.error") || type == QStringLiteral("folders.error")
        || type.startsWith(QStringLiteral("canvas.")) || type.startsWith(QStringLiteral("browser."))
        || type.startsWith(QStringLiteral("document.")) || !type.endsWith(QStringLiteral(".error")))
        return;
    const auto id = event.value(QStringLiteral("requestId")).toString();
    if (type == QStringLiteral("project.error")
        && event.value(QStringLiteral("operation")).toString() == QStringLiteral("project.act")) {
        if (!m_pending.contains(id))
            return;
        const auto pending = m_pending.take(id);
        m_projects.m_presentations[pending.projectId].insert(QStringLiteral("failure"), event.value(QStringLiteral("message")).toString());
        emit m_projects.presentationChanged();
        notifyBusy();
        return;
    }
    if (type == QStringLiteral("friends.error") && m_people.answerReport(event))
        return;

    auto quiet = false;
    const auto pendingIt = m_pending.constFind(id);
    if (pendingIt != m_pending.cend()) {
        const auto pending = pendingIt.value();
        quiet = pending.quiet;
        if (pending.operation == QStringLiteral("session.openRemote")
            && m_sessionActions.m_activeLink
            && m_sessionActions.m_activeLink->sessionId == pending.sessionId) {
            m_sessionActions.m_activeLink.reset();
            m_sessionActions.activatePendingLink();
        }
        if (pending.operation == QStringLiteral("project.open")
            && pending.projectId == m_projects.m_selectedId) {
            m_projects.open({});
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
    if (type == QStringLiteral("devices.error") && !id.isEmpty() && id == m_devices.m_checkRequest) {
        m_devices.m_checkRequest.clear();
        m_devices.m_checkedCode.clear();
        emit m_devices.changed();
    }
    if (quiet)
        return;
    const auto message = event.value(QStringLiteral("message")).toString();
    const auto operation = event.value(QStringLiteral("operation")).toString();
    if (!isRefresh(operation))
        setError(message);
    if (type == QStringLiteral("session.error") && operation == QStringLiteral("session.list")) {
        m_sessions.fail(message);
    }
    if (type == QStringLiteral("session.error") && operation == QStringLiteral("session.create")) {
        m_sessionActions.m_typeAhead.drop();
    }
}
}
