#include "presentation/MissionsModel.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonArray>
#include <QJsonValue>
#include <QGuiApplication>
#include <QClipboard>
#include <algorithm>

namespace kodosi {
MissionsModel::MissionsModel(Workspace& workspace, SessionCatalogModel& sessions)
    : isFront([] { return QGuiApplication::applicationState() == Qt::ApplicationActive; })
    , m_workspace(workspace)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authorityStateChanged, this, &MissionsModel::selectionChanged);
    connect(&workspace.m_desktop, &DesktopStateModel::activeViewChanged, this, &MissionsModel::readWatched);
}

bool MissionsModel::watching(const QString& roomId) const
{
    const auto view = m_presentations.value(roomId);
    return !roomId.isEmpty() && roomId == m_selectedId && m_workspace.m_desktop.activeView() == DesktopStateModel::Missions
        && view.value(QStringLiteral("conversation"), true).toBool() && view.value(QStringLiteral("followLatest"), true).toBool()
        && isFront();
}

void MissionsModel::readWatched()
{
    see(m_selectedId);
    seeCanvas();
}

void MissionsModel::see(const QString& roomId)
{
    if (!watching(roomId))
        return;
    quint64 latest = 0;
    for (const auto& value : m_rooms.value(roomId).value(QStringLiteral("messages")).toList())
        latest = std::max(latest, value.toMap().value(QStringLiteral("sequence")).toULongLong());
    if (latest == 0 || (latest <= m_seen.value(roomId) && !unreadRooms().contains(roomId)))
        return;
    m_seen.insert(roomId, latest);
    m_workspace.sendUntracked(QStringLiteral("mission.seen"),
        { { QStringLiteral("missionId"), roomId }, { QStringLiteral("sequence"), static_cast<qint64>(latest) } });
}

QStringList MissionsModel::unreadRooms() const
{
    QStringList result;
    for (const auto& value : m_missions) {
        const auto mission = value.toMap();
        if (mission.value(QStringLiteral("unread")).toBool())
            result.append(mission.value(QStringLiteral("id")).toString());
    }
    return result;
}

bool MissionsModel::busy() const
{
    return m_workspace.hasPending(Workspace::PendingDomain::Mission);
}

QStringList MissionsModel::sessionIds() const
{
    QStringList result;
    if (m_selectedId.isEmpty() || !m_sessions.hasAuthoritativeSnapshot())
        return result;
    for (int row = 0; row < m_sessions.rowCount(); ++row) {
        const auto index = m_sessions.index(row, 0);
        const auto id = m_sessions.data(index, SessionCatalogModel::SessionIdRole).toString();
        const auto session = m_sessions.session(id);
        if (session && session->missionId == m_selectedId && session->status != QStringLiteral("closing"))
            result.append(id);
    }
    return result;
}

void MissionsModel::create(const QString& name)
{
    if (!Workspace::validName(name)) {
        m_workspace.setError(tr("Use a name of 1–128 UTF-8 bytes."));
        return;
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Mission, QStringLiteral("mission.create"),
        { { QStringLiteral("name"), name.trimmed() } });
}

void MissionsModel::open(const QString& id)
{
    const bool changed = m_selectedId != id;
    m_selectedId = id;
    if (changed) {
        m_room = m_rooms.value(id); m_issues.clear(); m_issueRepository.clear();
        m_selected.clear(); m_members.clear();
        for (const auto& room : m_missions) { if (room.toMap().value(QStringLiteral("id")).toString() == id) { m_selected = room.toMap(); break; } }
    }
    emit presentationChanged();
    if (changed) emit roomChanged();
    emit selectionChanged();
    readWatched();
    for (auto it = m_workspace.m_pending.begin(); it != m_workspace.m_pending.end();) {
        if (it->operation == QStringLiteral("mission.open"))
            it = m_workspace.m_pending.erase(it);
        else
            ++it;
    }
    m_workspace.notifyBusy();
    if (!id.isEmpty()) {
        m_workspace.sendTracked(Workspace::PendingDomain::Mission, QStringLiteral("mission.open"),
            { { QStringLiteral("missionId"), id } }, {}, id);
    }
}

void MissionsModel::command(QString type, QJsonObject values)
{
    if (m_selectedId.isEmpty())
        return;
    values.insert(QStringLiteral("missionId"), m_selectedId);
    m_workspace.sendTracked(
        Workspace::PendingDomain::Mission, std::move(type), std::move(values), {}, m_selectedId);
}

void MissionsModel::rename(const QString& name)
{
    if (!Workspace::validName(name)) {
        m_workspace.setError(tr("Use a name of 1–128 UTF-8 bytes."));
        return;
    }
    command(QStringLiteral("mission.rename"), { { QStringLiteral("name"), name.trimmed() } });
}

void MissionsModel::remove()
{
    command(QStringLiteral("mission.delete"));
}

void MissionsModel::invite(const QString& userId)
{
    command(QStringLiteral("mission.invite"), { { QStringLiteral("userId"), userId } });
}

void MissionsModel::removeMember(const QString& userId)
{
    command(QStringLiteral("mission.removeMember"), { { QStringLiteral("userId"), userId } });
}

void MissionsModel::leave()
{
    command(QStringLiteral("mission.leave"));
}

void MissionsModel::acceptInvitation(const QString& id)
{
    QString room;
    for (const auto& entry : m_invitations) {
        if (entry.toMap().value(QStringLiteral("id")).toString() == id) { room = entry.toMap().value(QStringLiteral("missionId")).toString(); break; }
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Mission,
        QStringLiteral("mission.invitation.accept"),
        { { QStringLiteral("invitationId"), id } }, {}, room);
}

void MissionsModel::declineInvitation(const QString& id)
{
    m_workspace.sendTracked(Workspace::PendingDomain::Mission,
        QStringLiteral("mission.invitation.reject"),
        { { QStringLiteral("invitationId"), id } });
}

void MissionsModel::roomAction(const QVariantMap& action)
{
    if (m_selectedId.isEmpty()) return;
    setPresentation(QStringLiteral("failure"), QString {});
    if (action.value(QStringLiteral("type")) == QStringLiteral("issues")) {
        m_issues.clear(); m_issueRepository = action.value(QStringLiteral("repositoryId")).toString(); emit roomChanged();
    }
    m_workspace.sendTracked(Workspace::PendingDomain::Mission, QStringLiteral("room.command"),
        { { QStringLiteral("roomId"), m_selectedId }, { QStringLiteral("action"), QJsonObject::fromVariantMap(action) } }, {}, m_selectedId);
}

void MissionsModel::setPresentation(const QString& key, const QVariant& value)
{
    if (m_selectedId.isEmpty()) return;
    auto& state = m_presentations[m_selectedId];
    if (state.value(key) == value) return;
    state.insert(key, value);
    const auto group = key == QStringLiteral("taskTitle") || key == QStringLiteral("taskDescription")
            || key == QStringLiteral("taskRepositories") ? QStringLiteral("taskRevision")
        : key == QStringLiteral("message") ? QStringLiteral("messageRevision")
        : key == QStringLiteral("repositoryUrl") ? QStringLiteral("repositoryRevision") : QString {};
    if (!group.isEmpty()) state.insert(group, state.value(group).toULongLong() + 1);
    if ((key == QStringLiteral("conversation") || key == QStringLiteral("followLatest")) && value.toBool())
        state.remove(QStringLiteral("unread"));
    emit presentationChanged();
    if (key == QStringLiteral("conversation") || key == QStringLiteral("followLatest"))
        readWatched();
    if (key == QStringLiteral("canvas"))
        seeCanvas();
}

void MissionsModel::completeAction(const QString& room, const QString& action, const QVariantMap& submitted)
{
    auto& current = m_presentations[room];
    const auto clear = [&](const QString& revision, const QStringList& keys) {
        if (current.value(revision).toULongLong() != submitted.value(revision).toULongLong()) return;
        for (const auto& key : keys) current.remove(key);
    };
    if (action == QStringLiteral("post")) clear(QStringLiteral("messageRevision"), {QStringLiteral("message")});
    if (action == QStringLiteral("createTask"))
        clear(QStringLiteral("taskRevision"), {QStringLiteral("taskTitle"), QStringLiteral("taskDescription"), QStringLiteral("taskRepositories")});
    if (action == QStringLiteral("addRepository")) clear(QStringLiteral("repositoryRevision"), {QStringLiteral("repositoryUrl"), QStringLiteral("newRepository")});
    if (room == m_selectedId) emit presentationChanged();
}

void MissionsModel::copyAgentInstructions()
{
    QGuiApplication::clipboard()->setText(QStringLiteral("Read `kodosi room skill`, then use `kodosi --json room --room %1 context`. "
                                                         "You can read and post updates, create and pick up tasks, and share results for this work.")
                                              .arg(m_selectedId));
}

void MissionsModel::applyRoom(const QJsonObject& room)
{
    const auto roomId = room.value(QStringLiteral("roomId")).toString();
    const auto previous = m_rooms.value(roomId);
    auto next = room.toVariantMap();
    auto& view = m_presentations[roomId];
    const auto before = view;
    const bool shown = roomId == m_selectedId && m_workspace.m_desktop.activeView() == DesktopStateModel::Missions;
    if (!previous.isEmpty()) {
        quint64 latest = 0;
        for (const auto& value : previous.value(QStringLiteral("messages")).toList())
            latest = std::max(latest, value.toMap().value(QStringLiteral("sequence")).toULongLong());
        const auto userId = m_workspace.account().userId();
        const auto messages = room.value(QStringLiteral("messages")).toArray();
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
        const auto canvas = view.value(QStringLiteral("canvas")).toInt();
        auto fresh = view.value(QStringLiteral("fresh")).toList();
        const auto mark = [&](int section) {
            if (!(shown && canvas == section) && !fresh.contains(section))
                fresh.append(section);
        };
        if (keys(previous, QStringLiteral("tasks"), QStringLiteral("version")) != keys(next, QStringLiteral("tasks"), QStringLiteral("version")))
            mark(1);
        if (keys(previous, QStringLiteral("repositories"), QStringLiteral("id")) != keys(next, QStringLiteral("repositories"), QStringLiteral("id")))
            mark(2);
        if (!fresh.isEmpty())
            view.insert(QStringLiteral("fresh"), fresh);
    }
    m_rooms.insert(roomId, next);
    if (roomId == m_selectedId) {
        m_room = next;
        emit roomChanged();
        if (view != before)
            emit presentationChanged();
    }
    see(roomId);
}

void MissionsModel::seeCanvas()
{
    if (m_selectedId.isEmpty() || m_workspace.m_desktop.activeView() != DesktopStateModel::Missions)
        return;
    auto& view = m_presentations[m_selectedId];
    auto fresh = view.value(QStringLiteral("fresh")).toList();
    if (fresh.removeAll(view.value(QStringLiteral("canvas")).toInt()) == 0)
        return;
    view.insert(QStringLiteral("fresh"), fresh);
    emit presentationChanged();
}

void MissionsModel::reset()
{
    m_missions.clear();
    m_invitations.clear();
    m_selected.clear();
    m_members.clear();
    m_selectedId.clear(); m_room.clear(); m_rooms.clear(); m_presentations.clear(); m_issueRepository.clear(); m_issues.clear(); emit roomChanged(); emit presentationChanged();
    m_seen.clear();
    m_catalogTruncated = false;
    emit catalogChanged();
    emit selectionChanged();
}
}
