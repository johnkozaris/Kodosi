#include "presentation/SessionCatalogModel.hpp"
#include <QJsonArray>
#include <QSet>
#include <QUuid>
#include <algorithm>

namespace kodosi {
namespace {
// A Windows host writes `C:\…` or `\\server\share\…`, and Qt writes the same folder with `/`.
QString withSlashes(QString path)
{
    const bool drive = path.size() > 2 && path.at(0).unicode() < 0x80 && path.at(0).isLetter()
        && path.at(1) == u':' && (path.at(2) == u'\\' || path.at(2) == u'/');
    if (drive || path.startsWith(QStringLiteral("\\\\")))
        path.replace(u'\\', u'/');
    return path;
}
}

SessionCatalogModel::SessionCatalogModel(QObject* parent)
    : QAbstractListModel(parent)
{
    connect(this, &QAbstractItemModel::modelAboutToBeReset, this, [this] {
        m_sessionsCache.reset();
        m_listedCache.reset();
        m_groupsCache.reset();
    });
    connect(this, &QAbstractItemModel::modelReset, this, &SessionCatalogModel::folderGroupsChanged);
}
QVariantList SessionCatalogModel::folderGroups() const
{
    if (m_groupsCache)
        return *m_groupsCache;
    QMap<QString, QVariantMap> groups;
    for (const auto& session : m_sessions) {
        if (session.hidden || session.status == QStringLiteral("closing"))
            continue;
        const auto local = session.kind == QStringLiteral("local");
        const auto key = (local ? QStringLiteral("0:") : QStringLiteral("1:") + session.ownerUserId + QLatin1Char(':') + session.hostName)
            + QLatin1Char(':') + session.workingDirectory;
        auto& group = groups[key];
        const bool away = presentation(session).away && !local;
        const auto folder = session.workingDirectory.isEmpty() ? QString {} : folderName(session.workingDirectory);
        const auto name = folder.isEmpty() ? tr("Terminals") : folder;
        const auto host = local ? QString {} : session.hostName;
        const auto owner = local || session.isOwner ? QString {} : session.ownerName;
        QStringList label;
        for (const auto& part : { local ? QString {} : folder, owner, host }) {
            if (!part.isEmpty())
                label.append(part);
        }
        group.insert(QStringLiteral("offline"), away && group.value(QStringLiteral("offline"), true).toBool());
        group.insert(QStringLiteral("key"), key);
        group.insert(QStringLiteral("directory"), local ? withSlashes(session.workingDirectory) : QString {});
        group.insert(QStringLiteral("name"), name);
        group.insert(QStringLiteral("label"), label.isEmpty() ? name : label.join(QStringLiteral(" · ")));
        group.insert(QStringLiteral("host"), host);
        group.insert(QStringLiteral("owner"), owner);
        if (local && session.repository)
            group.insert(QStringLiteral("repository"), true);
        auto entries = group.value(QStringLiteral("sessions")).toList();
        entries.append(fields(session));
        group.insert(QStringLiteral("sessions"), entries);
    }
    QVariantList result;
    for (const auto& group : groups) result.append(group);
    m_groupsCache = result;
    return result;
}

QVariantList SessionCatalogModel::sessions() const
{
    if (m_sessionsCache)
        return *m_sessionsCache;
    QVariantList result;
    for (const auto& session : m_sessions) result.append(fields(session));
    m_sessionsCache = result;
    return result;
}

QVariantList SessionCatalogModel::listedSessions() const
{
    if (m_listedCache)
        return *m_listedCache;
    QVariantList result;
    for (const auto& session : m_sessions) {
        if (!session.hidden && session.status != QStringLiteral("closing"))
            result.append(fields(session));
    }
    m_listedCache = result;
    return result;
}

QStringList SessionCatalogModel::visible() const
{
    auto ids = m_views.keys();
    ids.sort();
    return ids;
}

void SessionCatalogModel::setVisible(const QString& id, bool visible)
{
    const auto views = m_views.value(id) + (visible ? 1 : -1);
    if (views > 0)
        m_views.insert(id, views);
    else
        m_views.remove(id);
    if (views == (visible ? 1 : 0))
        emit visibleChanged();
}

bool SessionCatalogModel::waiting(const QString& id) const
{
    const auto found = session(id);
    return found && !waitState(*found).isEmpty();
}

void SessionCatalogModel::alert(const QString& id)
{
    const auto found = session(id);
    if (!found)
        return;
    const bool calls = found->isOwner || presentation(*found).canControl;
    const auto words = report(*found);
    emit alertChanged(id, !calls || waitState(*found).isEmpty() ? QString {} : words.isEmpty() ? signLabel(*found) : words);
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
    if (session.status == QStringLiteral("closing"))
        return false;
    if (!session.programState.isEmpty())
        return session.programState == QStringLiteral("working");
    const auto scalars = session.title.toUcs4();
    return presentation(session).canType && !scalars.isEmpty() && isSpinner(scalars.first());
}

QString SessionCatalogModel::waitState(const Session& session)
{
    const bool result = session.programState == QStringLiteral("done") || session.programState == QStringLiteral("error");
    const bool blocked = session.programState == QStringLiteral("blocked") && session.status != QStringLiteral("closing");
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

QString SessionCatalogModel::folderName(const QString& path)
{
    const auto parts = withSlashes(path).split(u'/', Qt::SkipEmptyParts);
    return parts.isEmpty() ? path : parts.last();
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
    case MissionIdRole:
        return s.missionId;
    default:
        return {};
    }
}
std::optional<SessionCatalogModel::Session> SessionCatalogModel::session(const QString& id) const
{
    const auto found
        = std::find_if(m_sessions.cbegin(), m_sessions.cend(), [&](const auto& s) { return s.id == id; });
    return found == m_sessions.cend() ? std::nullopt : std::optional(*found);
}
QString SessionCatalogModel::current(const QString& id) const
{
    const auto found = std::find_if(m_sessions.cbegin(), m_sessions.cend(),
        [&](const auto& s) { return s.restoredFrom == id && s.kind == QStringLiteral("local"); });
    return found == m_sessions.cend() ? id : found->id;
}
std::optional<QString> SessionCatalogModel::restart(const QString& id, const QString& owner, const QString& host) const
{
    const auto found = std::find_if(m_sessions.cbegin(), m_sessions.cend(), [&](const auto& s) {
        return s.restoredFrom == id && s.ownerUserId == owner && s.hostDeviceId == host;
    });
    return found == m_sessions.cend() ? std::nullopt : std::optional(found->id);
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
    const bool away = s.status == QStringLiteral("offline");
    const bool live = s.status == QStringLiteral("running")
        || s.status == QStringLiteral("reconnecting") || away;
    const bool ready = live && (local || s.connectionState == QStringLiteral("connected")) && !blocked;
    const bool closing = s.status == QStringLiteral("closing");
    return { s.id, s.kind, !blocked && !closing, !local && !blocked && live, ready,
        ready && s.status == QStringLiteral("running"), away };
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
QString SessionCatalogModel::placeLabel(const Session& session)
{
    if (session.placeKind == QStringLiteral("container"))
        return tr("In the container %1").arg(session.placeName);
    if (session.placeKind == QStringLiteral("vm"))
        return tr("In the virtual machine %1").arg(session.placeName);
    return session.placeKind.isEmpty() ? QString {} : tr("On %1").arg(session.placeName);
}
QVariantMap SessionCatalogModel::fields(const Session& session) const
{
    const auto* s = &session;
    const auto p = presentation(session);
    const bool local = s->kind == QStringLiteral("local");
    return { { QStringLiteral("id"), s->id }, { QStringLiteral("name"), s->name },
        { QStringLiteral("activity"), activity(session) }, { QStringLiteral("working"), isWorking(session) },
        { QStringLiteral("progress"), isWorking(session) ? session.programProgress : -1 },
        { QStringLiteral("sign"), sign(session) }, { QStringLiteral("signLabel"), signLabel(session) },
        { QStringLiteral("asks"), waitState(session) == QStringLiteral("blocked") },
        { QStringLiteral("atPrompt"), local && p.canControl && session.prompt },
        { QStringLiteral("folderName"), folderName(s->workingDirectory) },
        { QStringLiteral("hostLabel"), local ? tr("This computer") : !s->hostName.isEmpty() ? s->hostName : !s->ownerName.isEmpty() ? s->ownerName : tr("Remote computer") },
        { QStringLiteral("ownerUserId"), s->ownerUserId },
        { QStringLiteral("kind"), s->kind }, { QStringLiteral("workingDirectory"), local ? withSlashes(s->workingDirectory) : QString {} },
        { QStringLiteral("displayDirectory"), s->workingDirectory },
        { QStringLiteral("program"), s->program }, { QStringLiteral("connectedUsers"), s->connectedUsers },
        { QStringLiteral("placeKind"), s->placeKind }, { QStringLiteral("placeName"), s->placeName },
        { QStringLiteral("placeLabel"), placeLabel(session) }, { QStringLiteral("root"), s->root },
        { QStringLiteral("ownerName"), s->ownerName }, { QStringLiteral("missionId"), s->missionId },
        { QStringLiteral("missionName"), s->missionName }, { QStringLiteral("status"), s->status }, { QStringLiteral("message"), s->message },
        { QStringLiteral("connectionState"), s->connectionState }, { QStringLiteral("isOwner"), s->isOwner },
        { QStringLiteral("sharedWith"), s->sharedWith }, { QStringLiteral("canControl"), p.canType },
        { QStringLiteral("canClose"), p.canType || (!local && p.away) }, { QStringLiteral("hidden"), s->hidden } };
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
            QStringLiteral("reconnecting"), QStringLiteral("offline"), QStringLiteral("closing") }
            .contains(s.status)
        || !QStringList { QStringLiteral("local"), QStringLiteral("connecting"), QStringLiteral("connected"),
            QStringLiteral("offline"), QStringLiteral("blocked") }
            .contains(s.connectionState)
        || !o.value(QStringLiteral("sharedWith")).isArray())
        return std::nullopt;
    for (const auto* key : {"ownerUserId", "ownerName", "hostDeviceId", "hostName", "missionName", "message", "workingDir", "title", "program", "restoredFrom"}) {
        const auto value = o.value(QLatin1String(key));
        if (!value.isNull() && !value.isUndefined() && (!value.isString() || value.toString().size()>4096)) return std::nullopt;
    }
    if (o.value(QStringLiteral("sharedWith")).toArray().size()>256) return std::nullopt;
    s.isOwner = o.value(QStringLiteral("isOwner")).toBool();
    s.attention = o.value(QStringLiteral("attention")).toBool();
    const auto hidden = o.value(QStringLiteral("hidden"));
    if (!hidden.isUndefined() && !hidden.isBool())
        return std::nullopt;
    s.hidden = hidden.toBool();
    s.workingDirectory = text("workingDir");
    s.title = text("title");
    s.program = text("program");
    const auto prompt = o.value(QStringLiteral("prompt"));
    if (!prompt.isUndefined() && !prompt.isBool())
        return std::nullopt;
    s.prompt = prompt.toBool();
    const auto repository = o.value(QStringLiteral("repository"));
    if (!repository.isUndefined() && !repository.isBool())
        return std::nullopt;
    s.repository = repository.toBool();
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
    const auto context = o.value(QStringLiteral("context"));
    if (!context.isUndefined() && !context.isNull()) {
        const auto reported = context.toObject();
        const auto root = reported.value(QStringLiteral("root"));
        const auto place = reported.value(QStringLiteral("place"));
        if (!context.isObject() || !root.isBool() || (!place.isUndefined() && !place.isObject()))
            return std::nullopt;
        s.root = root.toBool();
        if (place.isObject()) {
            s.placeKind = place.toObject().value(QStringLiteral("kind")).toString();
            s.placeName = place.toObject().value(QStringLiteral("name")).toString();
            if (!QStringList { QStringLiteral("remote"), QStringLiteral("container"), QStringLiteral("vm") }
                    .contains(s.placeKind)
                || s.placeName.isEmpty() || s.placeName.size() > 4096)
                return std::nullopt;
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
    s.hostDeviceId = text("hostDeviceId");
    s.hostName = text("hostName");
    s.missionId = text("missionId");
    s.missionName = text("missionName");
    s.message = text("message");
    s.restoredFrom = text("restoredFrom");
    if (!s.restoredFrom.isEmpty() && QUuid(s.restoredFrom).isNull())
        return std::nullopt;
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
    QStringList attention;
    QStringList hidden;
    for (const auto& next : result) {
        if (next.attention)
            attention.append(next.id);
        if (const auto before = session(next.id); next.hidden && before && !before->hidden)
            hidden.append(next.id);
    }
    const bool attentionMoved = attention != m_attention;
    // A program that animates its title sends a snapshot for each frame. When nothing
    // that Kodosi shows changed, keep the views as they are.
    if (m_state == Loaded && !attentionMoved && samePresentation(m_sessions, result)) {
        m_sessions = std::move(result);
        emit authoritativeSnapshotApplied();
        return;
    }
    const auto counted = m_sessions.size();
    beginResetModel();
    m_sessions = std::move(result);
    m_attention = std::move(attention);
    m_state = Loaded;
    m_error.clear();
    endResetModel();
    if (attentionMoved)
        emit attentionChanged();
    if (m_sessions.size() != counted)
        emit countChanged();
    emit authorityStateChanged();
    emit authoritativeSnapshotApplied();
    if (!hidden.isEmpty())
        emit sessionsHidden(hidden);
}
bool SessionCatalogModel::samePresentation(const QVector<Session>& before, const QVector<Session>& after) const
{
    return std::ranges::equal(before, after, [this](const Session& left, const Session& right) {
        return left.id == right.id && left.incarnationId == right.incarnationId && left.repository == right.repository
            && left.prompt == right.prompt && fields(left) == fields(right);
    });
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
