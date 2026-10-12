#include "presentation/MissionsModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonValue>
#include <QGuiApplication>
#include <QClipboard>
#include <algorithm>

namespace kodosi {
MissionsModel::MissionsModel(Workspace& workspace, SessionCatalogModel& sessions)
    : m_workspace(workspace)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authorityStateChanged, this, &MissionsModel::selectionChanged);
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
        if (m_sessions.data(index, SessionCatalogModel::MissionIdRole).toString() == m_selectedId)
            result.append(m_sessions.data(index, SessionCatalogModel::SessionIdRole).toString());
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
    const auto group = key.startsWith(QStringLiteral("task")) ? QStringLiteral("taskRevision")
        : key == QStringLiteral("message") ? QStringLiteral("messageRevision")
        : key == QStringLiteral("repositoryUrl") ? QStringLiteral("repositoryRevision") : QString {};
    if (!group.isEmpty()) state.insert(group, state.value(group).toULongLong() + 1);
    emit presentationChanged();
}

void MissionsModel::completeAction(const QString& room, const QString& action, const QVariantMap& submitted)
{
    auto& current = m_presentations[room];
    const auto clear = [&](const QString& revision, const QStringList& keys) {
        if (current.value(revision).toULongLong() != submitted.value(revision).toULongLong()) return;
        for (const auto& key : keys) current.remove(key);
    };
    if (action == QStringLiteral("post")) clear(QStringLiteral("messageRevision"), {QStringLiteral("message")});
    if (action == QStringLiteral("createTask")) clear(QStringLiteral("taskRevision"), {QStringLiteral("taskTitle"), QStringLiteral("taskDescription"), QStringLiteral("taskRepositories"), QStringLiteral("newTask")});
    if (action == QStringLiteral("addRepository")) clear(QStringLiteral("repositoryRevision"), {QStringLiteral("repositoryUrl"), QStringLiteral("newRepository")});
    if (room == m_selectedId) emit presentationChanged();
}

void MissionsModel::copyAgentInstructions()
{
    QGuiApplication::clipboard()->setText(QStringLiteral("Read `kodosi room skill`, then use `kodosi --json room --room %1 context` to participate in this room.").arg(m_selectedId));
}

void MissionsModel::applyRoom(const QJsonObject& room)
{
    const auto roomId = room.value(QStringLiteral("roomId")).toString();
    const auto previous = m_rooms.value(roomId);
    auto next = room.toVariantMap();
    if (previous.contains(QStringLiteral("hasOlder")))
        next.insert(QStringLiteral("hasOlder"), previous.value(QStringLiteral("hasOlder")).toBool() && next.value(QStringLiteral("hasOlder")).toBool());
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
    m_rooms.insert(roomId, next);
    if (roomId == m_selectedId) { m_room = next; emit roomChanged(); }
}

void MissionsModel::reset()
{
    m_missions.clear();
    m_invitations.clear();
    m_selected.clear();
    m_members.clear();
    m_selectedId.clear(); m_room.clear(); m_rooms.clear(); m_presentations.clear(); m_issueRepository.clear(); m_issues.clear(); emit roomChanged(); emit presentationChanged();
    m_catalogTruncated = false;
    emit catalogChanged();
    emit selectionChanged();
}
}
