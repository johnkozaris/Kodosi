#include "presentation/SessionActionsModel.hpp"
#include "presentation/AccountModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QFileInfo>
#include <QJsonArray>
#include <QJsonValue>
#include <QRandomGenerator>
#include <QUuid>

#include <algorithm>
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
    connect(&sessions, &SessionCatalogModel::sessionsHidden, this, [this](const QStringList& ids) {
        for (const auto& id : ids) minimize(id);
    });
    connect(&desktop, &DesktopStateModel::restored, this, [this](const QStringList& ids) {
        for (const auto& id : ids) {
            const auto session = m_sessions.session(id);
            if (session && !session->restoredFrom.isEmpty())
                m_workspace.projects().followTerminal(session->restoredFrom, id);
            if (session && session->kind == QStringLiteral("remote") && !opening(id))
                openRemote(id, true);
        }
    });
}

bool SessionActionsModel::opening(const QString& id) const
{
    return std::ranges::any_of(m_workspace.m_pending, [&id](const Workspace::Pending& pending) {
        return pending.operation == QStringLiteral("session.openRemote") && pending.sessionId == id;
    });
}

bool SessionActionsModel::openRemote(const QString& id, bool quiet)
{
    return m_workspace.sendTracked(Workspace::PendingDomain::Session, QStringLiteral("session.openRemote"),
        { { QStringLiteral("sessionId"), id } }, id, {}, quiet);
}

bool SessionActionsModel::busy() const
{
    return std::ranges::any_of(m_workspace.m_pending, [](const Workspace::Pending& pending) {
        return pending.domain == Workspace::PendingDomain::Session
            && pending.operation != QStringLiteral("session.close");
    });
}

void SessionActionsModel::refresh()
{
    m_sessions.beginRefresh();
    if (!m_workspace.sendUntracked(QStringLiteral("session.list")))
        m_sessions.fail(m_workspace.error());
    if (m_workspace.account().signedIn()) {
        m_workspace.sendUntracked(QStringLiteral("friends.refresh"));
        m_workspace.sendUntracked(QStringLiteral("devices.refresh"));
        m_workspace.sendUntracked(QStringLiteral("project.list"));
    }
    if (!m_workspace.projects().selectedProjectId().isEmpty())
        m_workspace.projects().open(m_workspace.projects().selectedProjectId());
}

bool SessionActionsModel::activate(const QString& id)
{
    return activateSession(id, false);
}

bool SessionActionsModel::activateSession(const QString& id, bool inProject)
{
    const auto session = m_sessions.session(id);
    if (!session && m_workspace.account().signedIn()) {
        if (opening(id))
            return true;
        if (QUuid(id).isNull()) {
            m_workspace.setError(tr("The terminal link is invalid."));
            return false;
        }
        m_activeLink = DeepLinkDestination { id };
        if (openRemote(id))
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
    QString leaving;
    if (!m_desktop.stagedSessionIds().contains(id) && m_desktop.stagedSessionIds().size() >= 6) {
        leaving = m_desktop.makingRoom(id);
        if (leaving.isEmpty())
            return m_desktop.stageSession(id, !inProject);
    }
    if (session->kind == QStringLiteral("remote") && !opening(id) && !openRemote(id, true))
        return false;
    if (!leaving.isEmpty())
        minimize(leaving);
    if (!m_desktop.stageSession(id, !inProject))
        return false;
    emit activated(id);
    return true;
}

bool SessionActionsModel::activateInProject(const QString& id)
{
    const bool opened = activateSession(id, true);
    if (opened) {
        m_workspace.projects().setPresentation(QStringLiteral("terminal"), id);
        m_workspace.projects().setPresentation(QStringLiteral("section"), 0);
        m_desktop.setActiveView(DesktopStateModel::Projects);
    }
    return opened;
}

bool SessionActionsModel::takeCreated(const QString& id)
{
    return m_created.remove(id);
}

void SessionActionsModel::giveTypedKeys(const QString& id)
{
    m_typeAhead.deliver(id);
}

void SessionActionsModel::completeCreation(const QString& id, const QString& projectId)
{
    if (m_created.size() > 64)
        m_created.clear();
    m_created.insert(id);
    m_typeAhead.expect(id);
    if (projectId.isEmpty()) {
        if (m_sessions.containsSession(id)) activate(id);
        else m_activeLink = DeepLinkDestination {id};
        return;
    }
    if (!m_sessions.containsSession(id)) { m_projectCreations.insert(id, projectId); return; }
    m_projectCreations.remove(id);
    if (m_workspace.projects().selectedProjectId() == projectId) activateInProject(id);
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
    dropRemote(id);
    return true;
}

void SessionActionsModel::dropRemote(const QString& id)
{
    const auto session = m_sessions.session(id);
    if (session && session->kind == QStringLiteral("remote") && !m_desktop.stagedSessionIds().contains(id)) {
        m_workspace.sendUntracked(
            QStringLiteral("session.disconnect"), { { QStringLiteral("sessionId"), id } });
    }
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

bool SessionActionsModel::create(const QString& name, const QString& directory, const QString& command,
    const QString& prompt, const QString& projectId, const QVariantMap& agent, const QString& computer)
{
    QJsonObject line;
    if (!command.trimmed().isEmpty())
        line.insert(QStringLiteral("command"), command.trimmed());
    if (!agent.isEmpty())
        line.insert(QStringLiteral("agent"), QJsonObject::fromVariantMap(agent));
    if (!prompt.trimmed().isEmpty())
        line.insert(QStringLiteral("prompt"), prompt.trimmed());
    if (!computer.isEmpty())
        line.insert(QStringLiteral("computer"), computer);
    if (!createInternal(name, directory, {}, {}, projectId, {}, line))
        return false;
    if (computer.isEmpty())
        m_settings.setWorkingDirectory(directory);
    return true;
}

bool SessionActionsModel::createQuick()
{
    return openQuick({});
}

bool SessionActionsModel::createSameAgent()
{
    return openQuick(selectedAgent());
}

bool SessionActionsModel::openQuick(const QVariantMap& agent)
{
    if (m_typeAhead.holding())
        return false;
    if (m_desktop.stagedSessionIds().size() >= 6 && m_desktop.stageLayoutMode() != DesktopStateModel::Focus) {
        m_workspace.setError(tr("Six terminals are open. Minimize one to open another."));
        return false;
    }
    const auto selected = m_sessions.session(m_desktop.selectedSessionId());
    const auto folder = selected && selected->kind == QStringLiteral("local") ? selected->workingDirectory : QString();
    if (!create({}, QFileInfo(folder).isDir() ? folder : m_settings.effectiveWorkingDirectory(), {}, {}, {}, agent))
        return false;
    m_typeAhead.hold();
    return true;
}

// What Kodosi does not know of the running agent stays the launcher's last choice.
QVariantMap SessionActionsModel::selectedAgent() const
{
    const auto selected = m_sessions.session(m_desktop.selectedSessionId());
    if (!selected || selected->kind != QStringLiteral("local") || selected->program.isEmpty())
        return {};
    QVariantMap agent { { QStringLiteral("agent"), selected->program } };
    const auto accounts = m_accounts ? m_accounts(selected->program) : QStringList {};
    if (selected->programAccount >= 0 && selected->programAccount < accounts.size())
        agent.insert(QStringLiteral("account"), accounts.at(selected->programAccount));
    const auto& settings = selected->programSettings;
    const auto& pending = selected->programPending;
    const auto take = [&](const QString& key, bool known) {
        if (pending.contains(key))
            agent.insert(key, pending.value(key));
        else if (settings.contains(key))
            agent.insert(key, settings.value(key));
        else if (known)
            agent.insert(key, QString());
    };
    take(QStringLiteral("model"), !settings.isEmpty());
    if (pending.contains(QStringLiteral("model")))
        agent.insert(QStringLiteral("effort"), pending.value(QStringLiteral("effort"), QString()));
    else
        take(QStringLiteral("effort"), !settings.isEmpty());
    take(QStringLiteral("permissions"), false);
    take(QStringLiteral("output"), false);
    return agent;
}

bool SessionActionsModel::createInternal(const QString& name, const QString& directory,
    const QString& provider, const QString& nativeConversationId, const QString& projectId,
    const QString& branch, const QJsonObject& line)
{
    QString title = name.trimmed();
    if (title.isEmpty())
        title = generatedName();
    if (!Workspace::validName(title)) {
        m_workspace.setError(tr("Use a name of 1 to 128 UTF-8 bytes."));
        return false;
    }
    QJsonObject values { { QStringLiteral("name"), title } };
    if (!projectId.isEmpty()) values.insert(QStringLiteral("projectId"), projectId);
    if (!directory.isEmpty())
        values.insert(QStringLiteral("workingDir"), directory);
    if (!branch.isEmpty()) {
        values.insert(QStringLiteral("branch"), branch);
        if (!m_settings.branchFolder().isEmpty())
            values.insert(QStringLiteral("worktrees"), m_settings.branchFolder());
    }
    for (auto field = line.begin(); field != line.end(); ++field)
        values.insert(field.key(), field.value());
    if (m_startSize && projectId.isEmpty() && !line.contains(QStringLiteral("computer"))) {
        if (const auto size = m_startSize(); !size.isEmpty())
            values.insert(QStringLiteral("size"), size);
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
        Workspace::PendingDomain::Session, QStringLiteral("session.create"), values, {}, projectId);
}

bool SessionActionsModel::command(
    QString type, const QString& id, QJsonObject values, bool ownerOnly)
{
    const auto session = m_sessions.session(id);
    if (!m_sessions.hasAuthoritativeSnapshot() || !session
        || session->connectionState == QStringLiteral("blocked")
        || (ownerOnly && !session->isOwner)) {
        m_workspace.setError(tr("You cannot do this in the selected terminal."));
        return false;
    }
    values.insert(QStringLiteral("sessionId"), id);
    values.insert(QStringLiteral("expectedRuntimeIncarnationId"), session->incarnationId);
    return m_workspace.sendTracked(
        Workspace::PendingDomain::Session, std::move(type), std::move(values), id);
}

bool SessionActionsModel::close(const QString& id)
{
    if (!command(QStringLiteral("session.close"), id))
        return false;
    m_desktop.noteClosing(id);
    return true;
}

bool SessionActionsModel::hide(const QString& id)
{
    return command(QStringLiteral("session.hide"), id);
}

bool SessionActionsModel::rename(const QString& id, const QString& name)
{
    const auto session = m_sessions.session(id);
    if (!Workspace::validName(name) || (session && session->name == name.trimmed()))
        return false;
    return command(QStringLiteral("session.rename"), id,
        { { QStringLiteral("name"), name.trimmed() } });
}

bool SessionActionsModel::startAgent(const QString& id, const QString& agent)
{
    return command(QStringLiteral("session.run"), id,
        { { QStringLiteral("agent"), QJsonObject { { QStringLiteral("agent"), agent } } } });
}

bool SessionActionsModel::changeAgent(const QString& id, const QVariantMap& change)
{
    return !change.isEmpty()
        && command(QStringLiteral("session.agent"), id, { { QStringLiteral("change"), QJsonObject::fromVariantMap(change) } });
}

bool SessionActionsModel::input(const QString& id, const QString& text)
{
    return !text.isEmpty() && command(QStringLiteral("session.input"), id, { { QStringLiteral("text"), text } });
}

bool SessionActionsModel::pastePicture(const QString& id, const QString& path)
{
    return !path.isEmpty() && command(QStringLiteral("session.picture"), id, { { QStringLiteral("path"), path } });
}

bool SessionActionsModel::share(
    const QString& id, const QStringList& users, const QStringList& expectedUsers)
{
    const auto session = m_sessions.session(id);
    if (!session || session->kind != QStringLiteral("local")) {
        m_workspace.setError(tr("Change sharing on the computer that hosts this terminal."));
        return false;
    }
    if (!m_workspace.account().signedIn()) {
        m_workspace.setError(tr("Sign in before you share a terminal."));
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

bool SessionActionsModel::attachProject(const QString& id, const QString& projectId)
{
    return command(QStringLiteral("session.attachProject"), id,
        { { QStringLiteral("projectId"),
            projectId.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(projectId) } },
        true);
}

void SessionActionsModel::route(const DeepLinkDestination& destination)
{
    if ((m_activeLink && m_activeLink->sessionId == destination.sessionId)
        || std::ranges::any_of(
            m_links, [&](const auto& link) { return link.sessionId == destination.sessionId; }))
        return;
    if (m_links.size() >= 32) {
        m_workspace.setError(tr("Too many terminal links wait to open."));
        return;
    }
    m_links.append(destination);
    activatePendingLink();
}

void SessionActionsModel::activatePendingLink()
{
    for (const auto& id : m_projectCreations.keys()) {
        if (m_sessions.containsSession(id)) completeCreation(id, m_projectCreations.value(id));
    }
    if (!m_activeLink && !m_links.isEmpty())
        m_activeLink = m_links.takeFirst();
    if (!m_activeLink)
        return;
    const auto id = m_activeLink->sessionId;
    if (m_sessions.containsSession(id)) {
        m_activeLink.reset();
        if (!activate(id))
            dropRemote(id);
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
    m_typeAhead.drop();
    if (clearLinks) {
        m_projectCreations.clear();
        m_activeLink.reset();
        m_links.clear();
    }
}
}
