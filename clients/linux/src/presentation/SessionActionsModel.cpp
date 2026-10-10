#include "presentation/SessionActionsModel.hpp"
#include "presentation/AccountModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonArray>
#include <QJsonValue>
#include <QRandomGenerator>
#include <QUuid>

#include <ranges>

namespace kodosi {
SessionActionsModel::SessionActionsModel(Workspace& workspace, SessionCatalogModel& sessions,
    DesktopStateModel& desktop, DesktopSettings& settings)
    : m_workspace(workspace)
    , m_sessions(sessions)
    , m_desktop(desktop)
    , m_settings(settings)
{
    connect(
        &sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &SessionActionsModel::activatePendingLink);
}

bool SessionActionsModel::busy() const
{
    return m_workspace.hasPending(Workspace::PendingDomain::Session);
}

void SessionActionsModel::refresh()
{
    m_sessions.beginRefresh();
    if (!m_workspace.sendUntracked(QStringLiteral("session.list")))
        m_sessions.fail(m_workspace.error());
    if (m_workspace.account().signedIn()) {
        m_workspace.sendUntracked(QStringLiteral("friends.refresh"));
        m_workspace.sendUntracked(QStringLiteral("devices.refresh"));
        m_workspace.sendUntracked(QStringLiteral("mission.list"));
    }
    if (!m_workspace.missions().selectedMissionId().isEmpty())
        m_workspace.missions().open(m_workspace.missions().selectedMissionId());
}

bool SessionActionsModel::activate(const QString& id)
{
    return activateSession(id, false);
}

bool SessionActionsModel::activateSession(const QString& id, bool inRoom)
{
    const auto session = m_sessions.session(id);
    if (!session && m_workspace.account().signedIn()) {
        for (const auto& pending : m_workspace.m_pending) {
            if (pending.operation == QStringLiteral("session.openRemote") && pending.sessionId == id)
                return true;
        }
        if (QUuid(id).isNull()) {
            m_workspace.setError(tr("The terminal link is invalid."));
            return false;
        }
        m_activeLink = DeepLinkDestination { id };
        if (m_workspace.sendTracked(Workspace::PendingDomain::Session,
                QStringLiteral("session.openRemote"), { { QStringLiteral("sessionId"), id } }, id))
            return true;
        m_activeLink.reset();
        return false;
    }
    if (!session || session->connectionState == QStringLiteral("blocked")
        || session->status == QStringLiteral("closing")) {
        m_workspace.setError(
            session && !session->message.isEmpty() ? session->message : tr("This terminal is no longer available."));
        return false;
    }
    if (!m_desktop.stagedSessionIds().contains(id) && m_desktop.stagedSessionIds().size() >= 6)
        return m_desktop.stageSession(id, !inRoom);
    if (session->kind == QStringLiteral("remote")) {
        for (const auto& pending : m_workspace.m_pending) {
            if (pending.operation == QStringLiteral("session.openRemote") && pending.sessionId == id) {
                if (!m_desktop.stageSession(id, !inRoom))
                    return false;
                emit activated(id);
                return true;
            }
        }
        if (!m_workspace.sendTracked(Workspace::PendingDomain::Session,
                QStringLiteral("session.openRemote"), { { QStringLiteral("sessionId"), id } }, id))
            return false;
    }
    if (!m_desktop.stageSession(id, !inRoom))
        return false;
    emit activated(id);
    return true;
}

bool SessionActionsModel::activateInRoom(const QString& id)
{
    const bool opened = activateSession(id, true);
    if (opened) {
        m_workspace.missions().setPresentation(QStringLiteral("terminal"), id);
        m_workspace.missions().setPresentation(QStringLiteral("canvas"), 0);
        m_desktop.setActiveView(DesktopStateModel::Missions);
    }
    return opened;
}

bool SessionActionsModel::createInRoom(const QString& roomId, const QString& directory)
{
    return createInternal({}, directory, {}, {}, roomId);
}

bool SessionActionsModel::takeCreated(const QString& id)
{
    return m_created.remove(id);
}

void SessionActionsModel::completeCreation(const QString& id, const QString& roomId)
{
    if (m_created.size() > 64)
        m_created.clear();
    m_created.insert(id);
    if (roomId.isEmpty()) {
        if (m_sessions.containsSession(id)) activate(id);
        else m_activeLink = DeepLinkDestination {id};
        return;
    }
    if (!m_sessions.containsSession(id)) { m_roomCreations.insert(id, roomId); return; }
    m_roomCreations.remove(id);
    if (m_workspace.missions().selectedMissionId() == roomId) activateInRoom(id);
}

bool SessionActionsModel::minimize(const QString& id)
{
    if (!m_desktop.unstageSession(id))
        return false;
    if (m_activeLink && m_activeLink->sessionId == id)
        m_activeLink.reset();
    for (auto it = m_workspace.m_pending.begin(); it != m_workspace.m_pending.end();) {
        if (it->operation == QStringLiteral("session.openRemote") && it->sessionId == id)
            it = m_workspace.m_pending.erase(it);
        else
            ++it;
    }
    m_workspace.notifyBusy();
    const auto session = m_sessions.session(id);
    if (session && session->kind == QStringLiteral("remote")) {
        m_workspace.sendUntracked(
            QStringLiteral("session.disconnect"), { { QStringLiteral("sessionId"), id } });
    }
    return true;
}

bool SessionActionsModel::create(const QString& name, const QString& directory)
{
    return createInternal(name, directory, {}, {});
}

bool SessionActionsModel::resume(
    const QString& provider, const QString& nativeConversationId, const QString& workingDirectory)
{
    if (!createInternal({}, workingDirectory, provider, nativeConversationId))
        return false;
    m_settings.setWorkingDirectory(workingDirectory);
    return true;
}

namespace {
QString generatedName()
{
    const QStringList adjectives { QStringLiteral("Amber"), QStringLiteral("Cedar"),
        QStringLiteral("Copper"), QStringLiteral("Jade"), QStringLiteral("Pearl"),
        QStringLiteral("Silver"), QStringLiteral("Slate"), QStringLiteral("Willow") };
    const QStringList animals { QStringLiteral("Falcon"), QStringLiteral("Fox"),
        QStringLiteral("Gecko"), QStringLiteral("Heron"), QStringLiteral("Lynx"),
        QStringLiteral("Otter"), QStringLiteral("Quail"), QStringLiteral("Wren") };
    return adjectives.at(QRandomGenerator::global()->bounded(adjectives.size()))
        + QLatin1Char(' ')
        + animals.at(QRandomGenerator::global()->bounded(animals.size()));
}
}

QString SessionActionsModel::suggestedBranch() const
{
    return generatedName().toLower().replace(QLatin1Char(' '), QLatin1Char('-'));
}

bool SessionActionsModel::createOnBranch(const QString& directory, const QString& branch)
{
    const auto name = branch.trimmed();
    return !directory.isEmpty() && !name.isEmpty() && createInternal(name, directory, {}, {}, {}, name);
}

bool SessionActionsModel::createInternal(const QString& name, const QString& directory,
    const QString& provider, const QString& nativeConversationId, const QString& roomId,
    const QString& branch)
{
    QString title = name.trimmed();
    if (title.isEmpty())
        title = generatedName();
    if (!Workspace::validName(title)) {
        m_workspace.setError(tr("Use a name of 1–128 UTF-8 bytes."));
        return false;
    }
    QJsonObject values { { QStringLiteral("name"), title } };
    if (!roomId.isEmpty()) values.insert(QStringLiteral("missionId"), roomId);
    if (!directory.isEmpty())
        values.insert(QStringLiteral("workingDir"), directory);
    if (!branch.isEmpty()) {
        values.insert(QStringLiteral("branch"), branch);
        if (!m_settings.branchFolder().isEmpty())
            values.insert(QStringLiteral("worktrees"), m_settings.branchFolder());
    }
    if (!nativeConversationId.isEmpty()) {
        if (provider != QStringLiteral("claude") && provider != QStringLiteral("copilot")) {
            m_workspace.setError(tr("Choose a supported provider."));
            return false;
        }
        values.insert(QStringLiteral("resume"),
            QJsonObject { { QStringLiteral("provider"), provider },
                { QStringLiteral("nativeConversationId"), nativeConversationId } });
    }
    return m_workspace.sendTracked(
        Workspace::PendingDomain::Session, QStringLiteral("session.create"), values, {}, roomId);
}

bool SessionActionsModel::command(
    QString type, const QString& id, QJsonObject values, bool ownerOnly)
{
    const auto session = m_sessions.session(id);
    if (!m_sessions.hasAuthoritativeSnapshot() || !session
        || session->connectionState == QStringLiteral("blocked")
        || (ownerOnly && !session->isOwner)) {
        m_workspace.setError(tr("This operation is not available for the selected terminal."));
        return false;
    }
    values.insert(QStringLiteral("sessionId"), id);
    values.insert(QStringLiteral("expectedRuntimeIncarnationId"), session->incarnationId);
    return m_workspace.sendTracked(
        Workspace::PendingDomain::Session, std::move(type), std::move(values), id);
}

bool SessionActionsModel::close(const QString& id)
{
    return command(QStringLiteral("session.close"), id);
}

bool SessionActionsModel::rename(const QString& id, const QString& name)
{
    const auto session = m_sessions.session(id);
    if (!Workspace::validName(name) || (session && session->name == name.trimmed()))
        return false;
    return command(QStringLiteral("session.rename"), id,
        { { QStringLiteral("name"), name.trimmed() } });
}

bool SessionActionsModel::run(const QString& id, const QString& line)
{
    const auto text = line.trimmed();
    const bool printable = std::ranges::all_of(text.toUcs4(), [](const char32_t scalar) { return QChar::isPrint(scalar); });
    if (text.isEmpty() || text.toUtf8().size() > 1024 || !printable)
        return false;
    return command(QStringLiteral("session.run"), id, { { QStringLiteral("command"), text } });
}

bool SessionActionsModel::share(
    const QString& id, const QStringList& users, const QStringList& expectedUsers)
{
    const auto session = m_sessions.session(id);
    if (!session || session->kind != QStringLiteral("local")) {
        m_workspace.setError(tr("Change sharing on the computer hosting this terminal."));
        return false;
    }
    if (!m_workspace.account().signedIn()) {
        m_workspace.setError(tr("Sign in before sharing a terminal."));
        return false;
    }
    return command(QStringLiteral("session.share"), id,
        { { QStringLiteral("userIds"), QJsonArray::fromStringList(users) },
            { QStringLiteral("expectedUserIds"), QJsonArray::fromStringList(expectedUsers) } },
        true);
}

bool SessionActionsModel::leave(const QString& id)
{
    return command(QStringLiteral("session.leave"), id);
}

bool SessionActionsModel::attachMission(const QString& id, const QString& missionId)
{
    return command(QStringLiteral("session.attachMission"), id,
        { { QStringLiteral("missionId"),
            missionId.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(missionId) } },
        true);
}

void SessionActionsModel::route(const DeepLinkDestination& destination)
{
    if ((m_activeLink && m_activeLink->sessionId == destination.sessionId)
        || std::ranges::any_of(
            m_links, [&](const auto& link) { return link.sessionId == destination.sessionId; }))
        return;
    if (m_links.size() >= 32) {
        m_workspace.setError(tr("Too many pending terminal links."));
        return;
    }
    m_links.append(destination);
    activatePendingLink();
}

void SessionActionsModel::activatePendingLink()
{
    for (const auto& id : m_roomCreations.keys()) {
        if (m_sessions.containsSession(id)) completeCreation(id, m_roomCreations.value(id));
    }
    if (!m_activeLink && !m_links.isEmpty())
        m_activeLink = m_links.takeFirst();
    if (!m_activeLink)
        return;
    const auto id = m_activeLink->sessionId;
    if (m_sessions.containsSession(id)) {
        m_activeLink.reset();
        activate(id);
        activatePendingLink();
        return;
    }
    if (m_workspace.account().finalizing())
        return;
    if (!m_workspace.account().signedIn()) {
        m_workspace.setError(tr("Sign in to open the linked terminal."));
        m_desktop.setActiveView(DesktopStateModel::Settings);
        return;
    }
    activate(id);
}

void SessionActionsModel::reset(bool clearLinks)
{
    m_created.clear();
    if (clearLinks) {
        m_roomCreations.clear();
        m_activeLink.reset();
        m_links.clear();
    }
}
}
