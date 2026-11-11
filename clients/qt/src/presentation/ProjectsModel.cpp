#include "presentation/ProjectsModel.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonArray>
#include <QJsonValue>
#include <QGuiApplication>
#include <QClipboard>
#include <algorithm>

namespace kodosi {
ProjectsModel::ProjectsModel(Workspace& workspace, SessionCatalogModel& sessions)
    : isFront([] { return QGuiApplication::applicationState() == Qt::ApplicationActive; })
    , m_workspace(workspace)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authorityStateChanged, this, &ProjectsModel::selectionChanged);
    connect(&workspace.m_desktop, &DesktopStateModel::activeViewChanged, this, &ProjectsModel::readWatched);
}

bool ProjectsModel::watching(const QString& projectId) const
{
    const auto view = m_presentations.value(projectId);
    return !projectId.isEmpty() && projectId == m_selectedId && m_workspace.m_desktop.activeView() == DesktopStateModel::Projects
        && view.value(QStringLiteral("conversation"), true).toBool() && view.value(QStringLiteral("followLatest"), true).toBool()
        && isFront();
}

void ProjectsModel::readWatched()
{
    see(m_selectedId);
    seeSection();
}

void ProjectsModel::see(const QString& projectId)
{
    if (!watching(projectId))
        return;
    quint64 latest = 0;
    for (const auto& value : m_contents.value(projectId).value(QStringLiteral("messages")).toList())
        latest = std::max(latest, value.toMap().value(QStringLiteral("sequence")).toULongLong());
    if (latest == 0 || (latest <= m_seen.value(projectId) && !unreadProjects().contains(projectId)))
        return;
    m_seen.insert(projectId, latest);
    m_workspace.sendUntracked(QStringLiteral("project.seen"),
        { { QStringLiteral("projectId"), projectId }, { QStringLiteral("sequence"), static_cast<qint64>(latest) } });
}

QStringList ProjectsModel::unreadProjects() const
{
    QStringList result;
    for (const auto& value : m_projects) {
        const auto project = value.toMap();
        if (project.value(QStringLiteral("unread")).toBool())
            result.append(project.value(QStringLiteral("id")).toString());
    }
    return result;
}

bool ProjectsModel::busy() const
{
    return m_workspace.hasPending(Workspace::PendingDomain::Project);
}

QStringList ProjectsModel::sessionIds() const
{
    QStringList result;
    if (m_selectedId.isEmpty() || !m_sessions.hasAuthoritativeSnapshot())
        return result;
    for (int row = 0; row < m_sessions.rowCount(); ++row) {
        const auto index = m_sessions.index(row, 0);
        const auto id = m_sessions.data(index, SessionCatalogModel::SessionIdRole).toString();
        const auto session = m_sessions.session(id);
        if (session && session->projectId == m_selectedId && session->status != QStringLiteral("closing"))
            result.append(id);
    }
    return result;
}

void ProjectsModel::create(const QString& name)
{
    if (!Workspace::validName(name)) {
        m_workspace.setError(tr("Use a name of 1 to 128 UTF-8 bytes."));
        return;
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Project, QStringLiteral("project.create"),
        { { QStringLiteral("name"), name.trimmed() } });
}

void ProjectsModel::open(const QString& id)
{
    const bool changed = m_selectedId != id;
    m_selectedId = id;
    if (changed) {
        m_content = m_contents.value(id); m_issues.clear(); m_issueRepository.clear(); m_issuesByRepository.clear();
        m_repositoryChoices.clear(); emit repositoryChoicesChanged();
        m_selected.clear(); m_members.clear();
        for (const auto& project : m_projects) { if (project.toMap().value(QStringLiteral("id")).toString() == id) { m_selected = project.toMap(); break; } }
    }
    emit presentationChanged();
    if (changed) emit contentChanged();
    emit selectionChanged();
    readWatched();
    for (auto it = m_workspace.m_pending.begin(); it != m_workspace.m_pending.end();) {
        if (it->operation == QStringLiteral("project.open"))
            it = m_workspace.m_pending.erase(it);
        else
            ++it;
    }
    m_workspace.notifyBusy();
    if (!id.isEmpty()) {
        m_workspace.sendTracked(Workspace::PendingDomain::Project, QStringLiteral("project.open"),
            { { QStringLiteral("projectId"), id } }, {}, id);
    }
}

void ProjectsModel::command(QString type, QJsonObject values)
{
    if (m_selectedId.isEmpty())
        return;
    values.insert(QStringLiteral("projectId"), m_selectedId);
    m_workspace.sendTracked(
        Workspace::PendingDomain::Project, std::move(type), std::move(values), {}, m_selectedId);
}

void ProjectsModel::rename(const QString& name)
{
    if (!Workspace::validName(name)) {
        m_workspace.setError(tr("Use a name of 1 to 128 UTF-8 bytes."));
        return;
    }
    command(QStringLiteral("project.rename"), { { QStringLiteral("name"), name.trimmed() } });
}

void ProjectsModel::remove()
{
    command(QStringLiteral("project.delete"));
}

void ProjectsModel::invite(const QString& userId)
{
    command(QStringLiteral("project.invite"), { { QStringLiteral("userId"), userId } });
}

void ProjectsModel::removeMember(const QString& userId)
{
    command(QStringLiteral("project.removeMember"), { { QStringLiteral("userId"), userId } });
}

void ProjectsModel::leave()
{
    command(QStringLiteral("project.leave"));
}

void ProjectsModel::acceptInvitation(const QString& id)
{
    QString projectId;
    for (const auto& entry : m_invitations) {
        if (entry.toMap().value(QStringLiteral("id")).toString() == id) { projectId = entry.toMap().value(QStringLiteral("projectId")).toString(); break; }
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Project,
        QStringLiteral("project.invitation.accept"),
        { { QStringLiteral("invitationId"), id } }, {}, projectId);
}

void ProjectsModel::declineInvitation(const QString& id)
{
    m_workspace.sendTracked(Workspace::PendingDomain::Project,
        QStringLiteral("project.invitation.reject"),
        { { QStringLiteral("invitationId"), id } });
}

void ProjectsModel::act(const QVariantMap& action)
{
    if (m_selectedId.isEmpty()) return;
    setPresentation(QStringLiteral("failure"), QString {});
    if (action.value(QStringLiteral("type")) == QStringLiteral("issues")) {
        m_issues.clear(); m_issueRepository = action.value(QStringLiteral("repositoryId")).toString(); emit contentChanged();
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Project, QStringLiteral("project.act"),
        { { QStringLiteral("projectId"), m_selectedId }, { QStringLiteral("action"), QJsonObject::fromVariantMap(action) } }, {}, m_selectedId);
}

void ProjectsModel::listRepositories()
{
    if (m_selectedId.isEmpty()) return;
    m_repositoryChoices = { { QStringLiteral("loading"), true } };
    emit repositoryChoicesChanged();
    if (!m_workspace.sendUntracked(QStringLiteral("repositories.list"), { { QStringLiteral("projectId"), m_selectedId } })) {
        m_repositoryChoices = { { QStringLiteral("failure"), tr("Kodosi could not ask for the repositories.") } };
        emit repositoryChoicesChanged();
    }
}

void ProjectsModel::setPresentation(const QString& key, const QVariant& value)
{
    if (m_selectedId.isEmpty()) return;
    auto& state = m_presentations[m_selectedId];
    if (state.value(key) == value) return;
    state.insert(key, value);
    const auto group = key == QStringLiteral("taskTitle") || key == QStringLiteral("taskDescription")
            || key == QStringLiteral("taskRepositories") || key == QStringLiteral("taskIssueRepository") ? QStringLiteral("taskRevision")
        : key == QStringLiteral("message") ? QStringLiteral("messageRevision")
        : key == QStringLiteral("repositoryUrl") ? QStringLiteral("repositoryRevision") : QString {};
    if (!group.isEmpty()) state.insert(group, state.value(group).toULongLong() + 1);
    if ((key == QStringLiteral("conversation") || key == QStringLiteral("followLatest")) && value.toBool())
        state.remove(QStringLiteral("unread"));
    emit presentationChanged();
    if (key == QStringLiteral("conversation") || key == QStringLiteral("followLatest"))
        readWatched();
    if (key == QStringLiteral("section"))
        seeSection();
}

void ProjectsModel::followTerminal(const QString& replaced, const QString& id)
{
    bool changed = false;
    for (auto& state : m_presentations) {
        if (state.value(QStringLiteral("terminal")).toString() == replaced) {
            state.insert(QStringLiteral("terminal"), id);
            changed = true;
        }
    }
    if (changed)
        emit presentationChanged();
}

void ProjectsModel::completeAction(const QString& projectId, const QString& action, const QVariantMap& submitted)
{
    auto& current = m_presentations[projectId];
    const auto clear = [&](const QString& revision, const QStringList& keys) {
        if (current.value(revision).toULongLong() != submitted.value(revision).toULongLong()) return;
        for (const auto& key : keys) current.remove(key);
    };
    if (action == QStringLiteral("post")) clear(QStringLiteral("messageRevision"), {QStringLiteral("message")});
    if (action == QStringLiteral("createTask"))
        clear(QStringLiteral("taskRevision"), {QStringLiteral("taskTitle"), QStringLiteral("taskDescription"), QStringLiteral("taskRepositories"),
            QStringLiteral("taskIssueRepository")});
    if (action == QStringLiteral("addRepository")) clear(QStringLiteral("repositoryRevision"), {QStringLiteral("repositoryUrl"), QStringLiteral("newRepository")});
    if (projectId == m_selectedId) emit presentationChanged();
}

void ProjectsModel::copyAgentInstructions()
{
    QGuiApplication::clipboard()->setText(QStringLiteral("Read `kodosi skill`, then use `kodosi --json project --project %1 context`. "
                                                         "You can read and post updates, create and pick up tasks, and share results for this work.")
                                              .arg(m_selectedId));
}

void ProjectsModel::applyContent(const QJsonObject& content)
{
    const auto projectId = content.value(QStringLiteral("projectId")).toString();
    const auto previous = m_contents.value(projectId);
    auto next = content.toVariantMap();
    auto& view = m_presentations[projectId];
    const auto before = view;
    const bool shown = projectId == m_selectedId && m_workspace.m_desktop.activeView() == DesktopStateModel::Projects;
    if (!previous.isEmpty()) {
        quint64 latest = 0;
        for (const auto& value : previous.value(QStringLiteral("messages")).toList())
            latest = std::max(latest, value.toMap().value(QStringLiteral("sequence")).toULongLong());
        const auto userId = m_workspace.account().userId();
        const auto messages = content.value(QStringLiteral("messages")).toArray();
        const auto added = std::count_if(messages.cbegin(), messages.cend(), [&](const QJsonValue& value) {
            const auto message = value.toObject();
            const bool own = message.value(QStringLiteral("agent")).toString().isEmpty()
                && message.value(QStringLiteral("authorId")).toString() == userId;
            return message.value(QStringLiteral("sequence")).toInteger() > static_cast<qint64>(latest) && !own;
        });
        const bool following = view.value(QStringLiteral("conversation"), true).toBool()
            && view.value(QStringLiteral("followLatest"), true).toBool();
        if (added > 0 && !(shown && following))
            view.insert(QStringLiteral("unread"), view.value(QStringLiteral("unread")).toInt() + static_cast<int>(added));
    }
    if (previous.contains(QStringLiteral("hasOlder")))
        next.insert(QStringLiteral("hasOlder"), previous.value(QStringLiteral("hasOlder")).toBool() && next.value(QStringLiteral("hasOlder")).toBool());
    if (previous.value(QStringLiteral("historyTrimmed")).toBool())
        next.insert(QStringLiteral("historyTrimmed"), true);
    QMap<QString, QVariant> messages;
    for (const auto& value : previous.value(QStringLiteral("messages")).toList())
        messages.insert(value.toMap().value(QStringLiteral("id")).toString(), value);
    for (const auto& value : next.value(QStringLiteral("messages")).toList())
        messages.insert(value.toMap().value(QStringLiteral("id")).toString(), value);
    auto ordered = messages.values();
    std::sort(ordered.begin(), ordered.end(), [](const QVariant& a, const QVariant& b) {
        return a.toMap().value(QStringLiteral("sequence")).toULongLong() < b.toMap().value(QStringLiteral("sequence")).toULongLong();
    });
    next.insert(QStringLiteral("messages"), ordered);
    auto tasks = next.value(QStringLiteral("tasks")).toList();
    std::sort(tasks.begin(), tasks.end(), [](const QVariant& a, const QVariant& b) {
        return a.toMap().value(QStringLiteral("id")).toString() < b.toMap().value(QStringLiteral("id")).toString();
    });
    next.insert(QStringLiteral("tasks"), tasks);
    if (!previous.isEmpty()) {
        const auto keys = [](const QVariantMap& snapshot, const QString& list, const QString& field) {
            QStringList result;
            for (const auto& value : snapshot.value(list).toList()) {
                const auto entry = value.toMap();
                result.append(entry.value(QStringLiteral("id")).toString() + QLatin1Char(':') + entry.value(field).toString());
            }
            return result;
        };
        const auto section = view.value(QStringLiteral("section")).toInt();
        auto fresh = view.value(QStringLiteral("fresh")).toList();
        const auto mark = [&](int changed) {
            if (!(shown && section == changed) && !fresh.contains(changed))
                fresh.append(changed);
        };
        if (keys(previous, QStringLiteral("tasks"), QStringLiteral("version")) != keys(next, QStringLiteral("tasks"), QStringLiteral("version")))
            mark(1);
        if (keys(previous, QStringLiteral("repositories"), QStringLiteral("id")) != keys(next, QStringLiteral("repositories"), QStringLiteral("id")))
            mark(2);
        if (!fresh.isEmpty())
            view.insert(QStringLiteral("fresh"), fresh);
    }
    m_contents.insert(projectId, next);
    if (projectId == m_selectedId) {
        m_content = next;
        emit contentChanged();
        if (view != before)
            emit presentationChanged();
    }
    see(projectId);
}

void ProjectsModel::seeSection()
{
    if (m_selectedId.isEmpty() || m_workspace.m_desktop.activeView() != DesktopStateModel::Projects)
        return;
    auto& view = m_presentations[m_selectedId];
    auto fresh = view.value(QStringLiteral("fresh")).toList();
    if (fresh.removeAll(view.value(QStringLiteral("section")).toInt()) == 0)
        return;
    view.insert(QStringLiteral("fresh"), fresh);
    emit presentationChanged();
}

void ProjectsModel::reset()
{
    m_projects.clear();
    m_invitations.clear();
    m_selected.clear();
    m_members.clear();
    m_selectedId.clear(); m_content.clear(); m_contents.clear(); m_presentations.clear(); m_issueRepository.clear(); m_issues.clear(); m_issuesByRepository.clear(); m_repositoryChoices.clear(); emit contentChanged(); emit presentationChanged(); emit repositoryChoicesChanged();
    m_seen.clear();
    m_catalogTruncated = false;
    emit catalogChanged();
    emit selectionChanged();
}
}
