#include "presentation/SessionCatalogModel.hpp"
#include <QJsonArray>
#include <QSet>
#include <QDir>
#include <QUuid>
#include <algorithm>

namespace kodosi {
SessionCatalogModel::SessionCatalogModel(QObject* parent)
    : QAbstractListModel(parent)
{
    connect(this, &QAbstractItemModel::modelReset, this, &SessionCatalogModel::folderGroupsChanged);
}
QVariantList SessionCatalogModel::folderGroups() const
{
    QMap<QString, QVariantMap> groups;
    for (const auto& session : m_sessions) {
        const auto local = session.kind == QStringLiteral("local");
        const auto key = (local ? QStringLiteral("0:") : QStringLiteral("1:") + session.ownerUserId + QLatin1Char(':') + session.hostName)
            + QLatin1Char(':') + session.workingDirectory;
        auto& group = groups[key];
        group.insert(QStringLiteral("key"), key);
        group.insert(QStringLiteral("directory"), local ? session.workingDirectory : QString {});
        const auto folder = QDir(session.workingDirectory).dirName();
        group.insert(QStringLiteral("name"), session.workingDirectory.isEmpty() ? tr("Terminals") : folder);
        group.insert(QStringLiteral("host"), local ? QString {} : session.hostName);
        group.insert(QStringLiteral("owner"), local || session.isOwner ? QString {} : session.ownerName);
        auto entries = group.value(QStringLiteral("sessions")).toList();
        entries.append(fields(session));
        group.insert(QStringLiteral("sessions"), entries);
    }
    QVariantList result;
    for (const auto& group : groups) result.append(group);
    return result;
}

QVariantList SessionCatalogModel::sessions() const
{
    QVariantList result;
    for (const auto& session : m_sessions) result.append(fields(session));
    return result;
}

bool SessionCatalogModel::working() const
{
    return std::any_of(m_sessions.cbegin(), m_sessions.cend(), isWorking);
}

void SessionCatalogModel::clearAttention(const QString& id)
{
    if (m_attention.removeAll(id) > 0)
        emit attentionChanged();
}

void SessionCatalogModel::noteAttention(const QString& id)
{
    if (!containsSession(id) || m_attention.contains(id))
        return;
    m_attention.append(id);
    emit attentionChanged();
}

namespace {
bool isSpinner(const char32_t scalar)
{
    return scalar >= 0x2800 && scalar <= 0x28FF;
}

bool isStatusGlyph(const char32_t scalar)
{
    static constexpr char32_t glyphs[] { 0x2733, 0x2722, 0x2736, 0x273B, 0x273D, 0x00B7, 0x25CF, 0x25CB };
    return isSpinner(scalar) || std::ranges::find(glyphs, scalar) != std::end(glyphs);
}
}

bool SessionCatalogModel::isWorking(const Session& session)
{
    if (!presentation(session).canControl)
        return false;
    if (!session.programState.isEmpty())
        return session.programState == QStringLiteral("working");
    const auto scalars = session.title.toUcs4();
    return !scalars.isEmpty() && isSpinner(scalars.first());
}

QString SessionCatalogModel::waitState(const Session& session)
{
    const bool result = session.programState == QStringLiteral("done") || session.programState == QStringLiteral("error");
    const bool blocked = session.programState == QStringLiteral("blocked") && presentation(session).canControl;
    return result || blocked ? session.programState : QString {};
}

QString SessionCatalogModel::sign(const Session& session)
{
    const auto state = waitState(session);
    if (state == QStringLiteral("done"))
        return state;
    if (state == QStringLiteral("error"))
        return QStringLiteral("failed");
    if (state != QStringLiteral("blocked"))
        return {};
    if (session.programKind == QStringLiteral("question"))
        return QStringLiteral("question");
    return session.programKind == QStringLiteral("auth") ? QStringLiteral("key") : QStringLiteral("hand");
}

QString SessionCatalogModel::signLabel(const Session& session)
{
    const auto form = sign(session);
    if (form == QStringLiteral("hand"))
        return tr("Needs your approval");
    if (form == QStringLiteral("question"))
        return tr("Needs your answer");
    if (form == QStringLiteral("key"))
        return tr("Needs you to sign in");
    if (form == QStringLiteral("done"))
        return tr("Done");
    return form == QStringLiteral("failed") ? tr("Failed") : QString {};
}

QString SessionCatalogModel::report(const Session& session)
{
    QStringList parts;
    for (const auto& part : { session.programTitle, session.programMessage }) {
        if (!part.isEmpty())
            parts.append(part);
    }
    return parts.join(QStringLiteral(": "));
}

QString SessionCatalogModel::activity(const Session& session)
{
    if (const auto words = report(session); !words.isEmpty())
        return words;
    qsizetype units = 0;
    for (const auto scalar : session.title.toUcs4()) {
        if (!isStatusGlyph(scalar) && !QChar::isSpace(scalar))
            break;
        units += QChar::requiresSurrogates(scalar) ? 2 : 1;
    }
    const auto text = session.title.mid(units).simplified();
    return text == session.name ? QString {} : text;
}

int SessionCatalogModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : static_cast<int>(m_sessions.size());
}
QVariant SessionCatalogModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_sessions.size())
        return {};
    const auto& s = m_sessions[index.row()];
    switch (role) {
    case SessionIdRole:
        return s.id;
    case NameRole:
        return s.name;
    case KindRole:
        return s.kind;
    case HostRole:
        return s.kind == QStringLiteral("local") ? tr("This computer") : s.hostName;
    case StatusRole:
        return s.status;
    case MissionIdRole:
        return s.missionId;
    case MissionNameRole:
        return s.missionName;
    case IsOwnerRole:
        return s.isOwner;
    case ConnectionRole:
        return s.connectionState;
    default:
        return {};
    }
}
QHash<int, QByteArray> SessionCatalogModel::roleNames() const
{
    return { { SessionIdRole, "sessionId" }, { NameRole, "name" }, { KindRole, "kind" },
        { HostRole, "hostName" }, { StatusRole, "status" }, { MissionIdRole, "missionId" },
        { MissionNameRole, "missionName" }, { IsOwnerRole, "isOwner" }, { ConnectionRole, "connectionState" } };
}
std::optional<SessionCatalogModel::Session> SessionCatalogModel::session(const QString& id) const
{
    const auto found
        = std::find_if(m_sessions.cbegin(), m_sessions.cend(), [&](const auto& s) { return s.id == id; });
    return found == m_sessions.cend() ? std::nullopt : std::optional(*found);
}
bool SessionCatalogModel::containsSession(const QString& id) const
{
    return session(id).has_value();
}
std::optional<QString> SessionCatalogModel::incarnationForSession(const QString& id) const
{
    const auto s = session(id);
    return s ? std::optional(s->incarnationId) : std::nullopt;
}
SessionCatalogModel::PresentationSession SessionCatalogModel::presentation(const Session& s)
{
    const bool local = s.kind == QStringLiteral("local");
    const bool blocked = s.connectionState == QStringLiteral("blocked");
    const bool live = s.status == QStringLiteral("running")
        || s.status == QStringLiteral("reconnecting");
    const bool ready = live && (local || s.connectionState == QStringLiteral("connected")) && !blocked;
    return { s.id, s.kind, !blocked, !local && !blocked && live, ready };
}
std::optional<SessionCatalogModel::PresentationSession> SessionCatalogModel::presentationSession(
    const QString& id) const
{
    const auto s = session(id);
    return s ? std::optional(presentation(*s)) : std::nullopt;
}
QVariantMap SessionCatalogModel::presentationForSession(const QString& id) const
{
    const auto s = session(id);
    return s ? fields(*s) : QVariantMap {};
}
QVariantMap SessionCatalogModel::fields(const Session& session) const
{
    const auto* s = &session;
    const auto p = presentation(session);
    const bool local = s->kind == QStringLiteral("local");
    return { { QStringLiteral("sessionId"), s->id }, { QStringLiteral("id"), s->id }, { QStringLiteral("name"), s->name },
        { QStringLiteral("activity"), activity(session) }, { QStringLiteral("working"), isWorking(session) },
        { QStringLiteral("progress"), isWorking(session) ? session.programProgress : -1 },
        { QStringLiteral("sign"), sign(session) }, { QStringLiteral("signLabel"), signLabel(session) },
        { QStringLiteral("atPrompt"), local && p.canControl && session.prompt },
        { QStringLiteral("folderName"),
            s->workingDirectory.isEmpty() ? QString {} : QDir(s->workingDirectory).dirName() },
        { QStringLiteral("hostLabel"), local ? tr("This computer") : !s->hostName.isEmpty() ? s->hostName : !s->ownerName.isEmpty() ? s->ownerName : tr("Remote computer") },
        { QStringLiteral("ownerUserId"), s->ownerUserId },
        { QStringLiteral("kind"), s->kind }, { QStringLiteral("workingDirectory"), s->kind == QStringLiteral("local") ? s->workingDirectory : QString {} },
        { QStringLiteral("displayDirectory"), s->workingDirectory },
        { QStringLiteral("headerTitle"), s->title.isEmpty() || s->title == s->name ? s->name : s->name + QStringLiteral(" · ") + s->title },
        { QStringLiteral("program"), s->program }, { QStringLiteral("connectedUsers"), s->connectedUsers },
        { QStringLiteral("ownerName"), s->ownerName }, { QStringLiteral("hostName"), s->hostName },
        { QStringLiteral("missionId"), s->missionId }, { QStringLiteral("missionName"), s->missionName },
        { QStringLiteral("status"), s->status }, { QStringLiteral("message"), s->message },
        { QStringLiteral("connectionState"), s->connectionState }, { QStringLiteral("isOwner"), s->isOwner },
        { QStringLiteral("sharedWith"), s->sharedWith },
        { QStringLiteral("canRetainPresentation"), p.canRetainPresentation },
        { QStringLiteral("isRemoteConnectable"), p.isRemoteConnectable },
        { QStringLiteral("canControl"), p.canControl } };
}
std::optional<SessionCatalogModel::Session> SessionCatalogModel::decode(const QJsonObject& o)
{
    auto text = [&](const char* key) { return o.value(QLatin1String(key)).toString(); };
    Session s;
    s.id = text("id");
    s.incarnationId = text("incarnationId");
    s.name = text("name");
    s.kind = text("kind");
    s.status = text("status");
    s.connectionState = text("connectionState");
    if (QUuid(s.id).isNull() || QUuid(s.incarnationId).isNull() || s.name.isEmpty()
        || s.name.toUtf8().size() > 128
        || (s.kind != QStringLiteral("local") && s.kind != QStringLiteral("remote"))
        || !o.value(QStringLiteral("isOwner")).isBool()
        || !QStringList { QStringLiteral("running"),
            QStringLiteral("reconnecting"), QStringLiteral("closing") }
            .contains(s.status)
        || !QStringList { QStringLiteral("local"), QStringLiteral("connecting"), QStringLiteral("connected"),
            QStringLiteral("offline"), QStringLiteral("blocked") }
            .contains(s.connectionState)
        || !o.value(QStringLiteral("sharedWith")).isArray())
        return std::nullopt;
    for (const auto* key : {"ownerName", "hostName", "missionName", "message", "workingDir", "title", "program"}) {
        const auto value = o.value(QLatin1String(key));
        if (!value.isNull() && !value.isUndefined() && (!value.isString() || value.toString().size()>4096)) return std::nullopt;
    }
    if (o.value(QStringLiteral("sharedWith")).toArray().size()>256) return std::nullopt;
    s.isOwner = o.value(QStringLiteral("isOwner")).toBool();
    s.workingDirectory = text("workingDir");
    s.title = text("title");
    s.program = text("program");
    const auto prompt = o.value(QStringLiteral("prompt"));
    if (!prompt.isUndefined() && !prompt.isBool())
        return std::nullopt;
    s.prompt = prompt.toBool();
    const auto reported = o.value(QStringLiteral("programStatus"));
    if (!reported.isUndefined() && !reported.isNull()) {
        if (!reported.isObject())
            return std::nullopt;
        const auto report = reported.toObject();
        s.programState = report.value(QStringLiteral("state")).toString();
        if (!QStringList { QStringLiteral("idle"), QStringLiteral("working"), QStringLiteral("done"),
                QStringLiteral("blocked"), QStringLiteral("error") }
                .contains(s.programState))
            return std::nullopt;
        for (const auto* key : {"kind", "title", "message"}) {
            const auto value = report.value(QLatin1String(key));
            if (!value.isUndefined() && (!value.isString() || value.toString().size() > 4096))
                return std::nullopt;
        }
        s.programKind = report.value(QStringLiteral("kind")).toString();
        s.programTitle = report.value(QStringLiteral("title")).toString();
        s.programMessage = report.value(QStringLiteral("message")).toString();
        const auto progress = report.value(QStringLiteral("progress"));
        if (!progress.isUndefined()) {
            if (!progress.isDouble() || progress.toInt(-1) < 0 || progress.toInt(-1) > 100)
                return std::nullopt;
            s.programProgress = progress.toInt();
        }
    }
    const auto connected = o.value(QStringLiteral("connectedUsers"));
    if (!connected.isUndefined() && (!connected.isArray() || connected.toArray().size() > 128)) return std::nullopt;
    for (const auto& user : connected.toArray()) {
        if (!user.isString() || QUuid(user.toString()).isNull()) return std::nullopt;
        s.connectedUsers.append(user.toString());
    }
    s.ownerUserId = text("ownerUserId");
    s.ownerName = text("ownerName");
    s.hostName = text("hostName");
    s.missionId = text("missionId");
    s.missionName = text("missionName");
    s.message = text("message");
    s.createRequestId = text("createRequestId");
    for (const auto& v : o.value(QStringLiteral("sharedWith")).toArray()) {
        if (!v.isString() || QUuid(v.toString()).isNull())
            return std::nullopt;
        s.sharedWith.append(v.toString());
    }
    return s;
}
void SessionCatalogModel::apply(const QJsonObject& event)
{
    if (event.value(QStringLiteral("type")).toString() != QStringLiteral("sessions.snapshot"))
        return;
    if (!event.value(QStringLiteral("sessions")).isArray()
        || event.value(QStringLiteral("sessions")).toArray().size() > 4096) {
        fail(tr("The terminal list is invalid."));
        return;
    }
    QVector<Session> result;
    QSet<QString> ids;
    for (const auto& value : event.value(QStringLiteral("sessions")).toArray()) {
        const auto s = decode(value.toObject());
        if (!s || ids.contains(s->id)) {
            fail(tr("The terminal list contains an invalid entry."));
            return;
        }
        ids.insert(s->id);
        result.append(*s);
    }
    auto attention = m_attention;
    QVector<std::pair<QString, QString>> alerts;
    for (const auto& next : result) {
        const auto before = this->session(next.id);
        const bool stopped = before && isWorking(*before) && !isWorking(next);
        const bool waits = !waitState(next).isEmpty() && !(before && waitState(*before) == waitState(next));
        if ((stopped || waits) && !attention.contains(next.id))
            attention.append(next.id);
        if (before && waitState(*before) != waitState(next)) {
            const auto words = report(next);
            alerts.append({ next.id, !next.isOwner || waitState(next).isEmpty() ? QString {} : words.isEmpty() ? signLabel(next) : words });
        }
    }
    attention.removeIf([&](const QString& id) { return !ids.contains(id); });
    const bool attentionMoved = attention != m_attention;
    beginResetModel();
    m_sessions = std::move(result);
    m_attention = std::move(attention);
    m_state = Loaded;
    m_error.clear();
    endResetModel();
    if (attentionMoved)
        emit attentionChanged();
    emit countChanged();
    emit authorityStateChanged();
    emit authoritativeSnapshotApplied();
    for (const auto& [id, text] : std::as_const(alerts))
        emit alertChanged(id, text);
}
void SessionCatalogModel::resetRuntimeAuthority()
{
    beginResetModel();
    m_sessions.clear();
    m_attention.clear();
    endResetModel();
    emit attentionChanged();
    m_state = Loading;
    m_error.clear();
    emit countChanged();
    emit authorityStateChanged();
}
void SessionCatalogModel::beginRefresh()
{
    m_state = Loading;
    m_error.clear();
    emit authorityStateChanged();
}
void SessionCatalogModel::fail(QString error)
{
    m_state = Failed;
    m_error = std::move(error);
    emit authorityStateChanged();
}
}
