#include "presentation/DosiModel.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/DosiWords.hpp"
#include "presentation/MissionsModel.hpp"
#include "presentation/SessionActionsModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/Workspace.hpp"

#include <QJsonArray>
#include <QLocale>

#include <algorithm>

namespace kodosi {
namespace {
QVariantMap bodyOf(const QVariant& item)
{
    return item.toMap().value(QStringLiteral("body")).toMap();
}

QString kindOf(const QVariant& item)
{
    return bodyOf(item).value(QStringLiteral("kind")).toString();
}

QString turnOf(const QVariant& item)
{
    return item.toMap().value(QStringLiteral("turn")).toString();
}

QString idOf(const QVariant& item)
{
    return item.toMap().value(QStringLiteral("id")).toString();
}

bool openQuestion(const QVariantMap& content)
{
    return content.value(QStringLiteral("kind")).toString() == QStringLiteral("question")
        && content.value(QStringLiteral("answer")).isNull();
}

QJsonValue optional(const QString& value)
{
    return value.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(value);
}
}

DosiModel::DosiModel(Workspace& workspace, SessionCatalogModel& sessions)
    : m_workspace(workspace)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::folderGroupsChanged, this, &DosiModel::rebuild);
    connect(&workspace.m_missions, &MissionsModel::catalogChanged, this, &DosiModel::rebuild);
    connect(&workspace.m_desktop, &DesktopStateModel::dosiOpenChanged, this, &DosiModel::refreshSeen);
    connect(&workspace.m_desktop, &DesktopStateModel::sidebarOpenChanged, this, &DosiModel::refreshSeen);
}

QString DosiModel::state() const
{
    const auto value = m_snapshot.value(QStringLiteral("state")).toString();
    return value.isEmpty() ? QStringLiteral("unavailable") : value;
}

bool DosiModel::busy() const
{
    const auto value = state();
    return value == QStringLiteral("working") || value == QStringLiteral("stopping");
}

QString DosiModel::mood() const
{
    const auto value = state();
    if (value == QStringLiteral("unavailable"))
        return QStringLiteral("off");
    if (value == QStringLiteral("stopping"))
        return QStringLiteral("stopping");
    if (value != QStringLiteral("working")) {
        if (value == QStringLiteral("idle") && asking())
            return QStringLiteral("asking");
        if (value == QStringLiteral("idle") && !m_items.isEmpty() && kindOf(m_items.last()) == QStringLiteral("notice")
            && bodyOf(m_items.last()).value(QStringLiteral("tone")).toString() != QStringLiteral("stopped"))
            return QStringLiteral("stuck");
        return QStringLiteral("resting");
    }
    if (m_items.isEmpty())
        return QStringLiteral("thinking");
    const auto content = bodyOf(m_items.last());
    const auto last = content.value(QStringLiteral("kind")).toString();
    const auto live = content.value(QStringLiteral("live")).toBool();
    if (last == QStringLiteral("reasoning") && live)
        return QStringLiteral("thinking");
    if (last == QStringLiteral("text") && live)
        return QStringLiteral("talking");
    if (last == QStringLiteral("question"))
        return QStringLiteral("asking");
    if (last == QStringLiteral("tool") && content.value(QStringLiteral("status")).toString() == QStringLiteral("running")) {
        static const QStringList reads { QStringLiteral("look"), QStringLiteral("read"), QStringLiteral("wait"),
            QStringLiteral("search"), QStringLiteral("fetch") };
        return reads.contains(content.value(QStringLiteral("action")).toString()) ? QStringLiteral("reading")
                                                                                   : QStringLiteral("working");
    }
    return QStringLiteral("thinking");
}

QString DosiModel::line() const
{
    const auto value = state();
    if (value == QStringLiteral("unavailable"))
        return tr("Choose Dosi's agent in Settings");
    if (value == QStringLiteral("stopping"))
        return tr("Dosi is stopping");
    if (value == QStringLiteral("working")) {
        const auto computer = m_snapshot.value(QStringLiteral("remote")).toObject().value(QStringLiteral("computer")).toString();
        return computer.isEmpty() ? tr("Dosi is working") : tr("Dosi is working on %1").arg(computer);
    }
    if (asking())
        return tr("Dosi has a question");
    return unseen() ? tr("Dosi answered") : tr("Ask Dosi");
}

bool DosiModel::unseen() const
{
    return !busy() && !m_items.isEmpty() && turnOf(m_items.last()) != m_seenTurn
        && kindOf(m_items.last()) != QStringLiteral("person");
}

bool DosiModel::asking() const
{
    return std::any_of(m_items.cbegin(), m_items.cend(), [](const QVariant& item) { return openQuestion(bodyOf(item)); });
}

QVariantList DosiModel::touched() const
{
    QVariantList result;
    if (!busy() || m_items.isEmpty())
        return result;
    const auto turn = turnOf(m_items.last());
    QStringList keys;
    for (auto item = m_items.crbegin(); item != m_items.crend() && result.size() < 3; ++item) {
        if (turnOf(*item) != turn || kindOf(*item) != QStringLiteral("tool"))
            continue;
        const auto target = bodyOf(*item).value(QStringLiteral("target")).toMap();
        const auto kind = target.value(QStringLiteral("kind")).toString();
        const auto value = target.value(QStringLiteral("value")).toString();
        const auto key = kind + QLatin1Char(':') + value;
        if ((kind != QStringLiteral("terminal") && kind != QStringLiteral("room")) || keys.contains(key))
            continue;
        const auto id = kind == QStringLiteral("terminal") ? terminalId(value) : roomId(value);
        if (id.isEmpty())
            continue;
        keys.append(key);
        result.prepend(QVariantMap { { QStringLiteral("kind"), kind }, { QStringLiteral("id"), id }, { QStringLiteral("key"), key } });
    }
    return result;
}

QString DosiModel::liveTurn() const
{
    return busy() && !m_items.isEmpty() ? turnOf(m_items.last()) : QString {};
}

bool DosiModel::pending() const
{
    if (state() != QStringLiteral("working") || !m_snapshot.value(QStringLiteral("remote")).toObject().isEmpty())
        return false;
    if (m_items.isEmpty())
        return true;
    const auto content = bodyOf(m_items.last());
    const auto kind = content.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("question"))
        return !openQuestion(content);
    if (kind == QStringLiteral("tool"))
        return content.value(QStringLiteral("status")).toString() != QStringLiteral("running");
    return !content.value(QStringLiteral("live")).toBool();
}

QVariantMap DosiModel::reading() const
{
    if (!busy() || m_items.isEmpty())
        return {};
    const auto tool = bodyOf(m_items.last());
    static const QStringList reads { QStringLiteral("look"), QStringLiteral("read"), QStringLiteral("wait") };
    if (tool.value(QStringLiteral("kind")).toString() != QStringLiteral("tool")
        || tool.value(QStringLiteral("status")).toString() != QStringLiteral("running")
        || !reads.contains(tool.value(QStringLiteral("action")).toString()))
        return {};
    const auto target = tool.value(QStringLiteral("target")).toMap();
    if (target.isEmpty())
        return {};
    const auto targetKind = target.value(QStringLiteral("kind")).toString();
    const auto value = target.value(QStringLiteral("value")).toString();
    QVariantMap result { { QStringLiteral("item"), idOf(m_items.last()) }, { QStringLiteral("kind"), targetKind },
        { QStringLiteral("value"), value } };
    if (targetKind == QStringLiteral("terminal"))
        result.insert(QStringLiteral("sessionId"), terminalId(value));
    if (targetKind == QStringLiteral("room"))
        result.insert(QStringLiteral("roomId"), roomId(value));
    return result;
}

QVariantMap DosiModel::details() const
{
    QStringList agents;
    for (const auto& value : m_snapshot.value(QStringLiteral("agents")).toArray()) {
        const auto agent = value.toObject();
        if (agent.value(QStringLiteral("installed")).toBool())
            agents.append(agent.value(QStringLiteral("agent")).toString());
    }
    const auto shared = m_snapshot.value(QStringLiteral("shared")).toBool();
    const auto started = m_snapshot.value(QStringLiteral("conversation")).toObject().value(QStringLiteral("startedAt"));
    const auto place = shared ? tr("on all your computers") : tr("on this computer");
    const auto conversation = started.isDouble()
        ? tr("Started %1, %2").arg(QLocale().toString(QDateTime::fromMSecsSinceEpoch(started.toInteger()), QLocale::ShortFormat), place)
        : place;
    return {
        { QStringLiteral("remote"), m_snapshot.value(QStringLiteral("remote")).toObject().value(QStringLiteral("computer")).toString() },
        { QStringLiteral("earlier"), m_earlier },
        { QStringLiteral("loadingEarlier"), m_loadingEarlier },
        { QStringLiteral("queued"), m_snapshot.value(QStringLiteral("queued")).toArray().toVariantList() },
        { QStringLiteral("folder"), m_snapshot.value(QStringLiteral("folder")).toString() },
        { QStringLiteral("conversation"), conversation },
        { QStringLiteral("setup"), m_snapshot.value(QStringLiteral("setup")).toObject().toVariantMap() },
        { QStringLiteral("agents"), agents },
        { QStringLiteral("instructionsEdited"), m_snapshot.value(QStringLiteral("instructionsEdited")).toBool() },
    };
}

void DosiModel::setDraft(const QString& draft)
{
    if (m_draft == draft)
        return;
    m_draft = draft;
    emit draftChanged();
}

QJsonObject DosiModel::context() const
{
    const auto& desktop = m_workspace.m_desktop;
    QString terminal;
    QString room;
    if (desktop.activeView() == DesktopStateModel::Sessions) {
        terminal = desktop.selectedSessionId();
    } else if (desktop.activeView() == DesktopStateModel::Missions) {
        room = m_workspace.m_missions.selectedMissionId();
        if (!room.isEmpty())
            terminal = m_workspace.m_missions.presentation().value(QStringLiteral("terminal")).toString();
    }
    if (terminal.isEmpty() && room.isEmpty())
        return {};
    return { { QStringLiteral("terminal"), optional(terminal) }, { QStringLiteral("room"), optional(room) } };
}

void DosiModel::send(const QString& text)
{
    const bool typed = text.isEmpty();
    const auto message = (typed ? m_draft : text).trimmed();
    if (message.isEmpty() || state() == QStringLiteral("unavailable"))
        return;
    if (typed)
        setDraft({});
    QJsonObject values { { QStringLiteral("text"), message } };
    if (const auto where = context(); !where.isEmpty())
        values.insert(QStringLiteral("context"), where);
    request(QStringLiteral("dosi.send"), values, typed ? std::optional(message) : std::nullopt);
}

void DosiModel::stop()
{
    request(QStringLiteral("dosi.stop"));
}

void DosiModel::unqueue(const QString& id)
{
    request(QStringLiteral("dosi.unqueue"), { { QStringLiteral("queuedId"), id } });
}

void DosiModel::answer(const QString& itemId, const QStringList& choices)
{
    request(QStringLiteral("dosi.answer"),
        { { QStringLiteral("itemId"), itemId }, { QStringLiteral("choices"), QJsonArray::fromStringList(choices) } });
}

void DosiModel::renew()
{
    request(QStringLiteral("dosi.new"));
}

void DosiModel::choose(const QVariantMap& change)
{
    QJsonObject value { { QStringLiteral("type"), change.value(QStringLiteral("type")).toString() } };
    const QStringList keys = value.value(QStringLiteral("type")).toString() == QStringLiteral("pick")
        ? QStringList { QStringLiteral("agent"), QStringLiteral("account"), QStringLiteral("model"), QStringLiteral("effort") }
        : QStringList { QStringLiteral("agent") };
    for (const auto& key : keys)
        value.insert(key, optional(change.value(key).toString()));
    request(QStringLiteral("dosi.setup"), { { QStringLiteral("change"), value } }, QString {});
}

void DosiModel::loadInstructions()
{
    request(QStringLiteral("dosi.instructions.read"), {}, QString {});
}

void DosiModel::saveInstructions(const QString& text)
{
    m_instructions.insert(QStringLiteral("saving"), true);
    emit instructionsChanged();
    request(QStringLiteral("dosi.instructions.write"), { { QStringLiteral("text"), text } }, QString {});
}

void DosiModel::resetInstructions()
{
    m_instructions.insert(QStringLiteral("saving"), true);
    emit instructionsChanged();
    request(QStringLiteral("dosi.instructions.write"), { { QStringLiteral("text"), QJsonValue(QJsonValue::Null) } }, QString {});
}

void DosiModel::loadEarlier()
{
    if (!m_earlier || m_loadingEarlier || m_items.isEmpty())
        return;
    m_loadingEarlier = true;
    emit changed();
    const auto first = idOf(m_items.first());
    request(QStringLiteral("dosi.history"), { { QStringLiteral("before"), first } }, first);
}

void DosiModel::open(const QString& place, const QString& value)
{
    auto& desktop = m_workspace.m_desktop;
    if (place == QStringLiteral("terminal")) {
        if (const auto id = terminalId(value); !id.isEmpty())
            m_workspace.m_sessionActions.activate(id);
    } else if (place == QStringLiteral("room")) {
        if (const auto id = roomId(value); !id.isEmpty()) {
            desktop.setActiveView(DesktopStateModel::Missions);
            m_workspace.m_missions.open(id);
        }
    } else if (place == QStringLiteral("rooms")) {
        desktop.setActiveView(DesktopStateModel::Missions);
        m_workspace.m_missions.open({});
    } else if (place == QStringLiteral("terminals")) {
        desktop.setActiveView(DesktopStateModel::Sessions);
    }
}

QString DosiModel::terminalId(const QString& reference) const
{
    if (reference.isEmpty())
        return {};
    const auto sessions = m_sessions.sessions();
    const auto find = [&](auto matches) {
        for (const auto& value : sessions) {
            const auto session = value.toMap();
            if (matches(session))
                return session.value(QStringLiteral("id")).toString();
        }
        return QString {};
    };
    auto found = find([&](const QVariantMap& session) { return session.value(QStringLiteral("id")).toString() == reference; });
    if (found.isEmpty())
        found = find([&](const QVariantMap& session) {
            return session.value(QStringLiteral("name")).toString().compare(reference, Qt::CaseInsensitive) == 0;
        });
    if (found.isEmpty() && reference.size() >= 4)
        found = find([&](const QVariantMap& session) { return session.value(QStringLiteral("id")).toString().startsWith(reference); });
    return found;
}

QString DosiModel::roomId(const QString& reference) const
{
    if (reference.isEmpty())
        return {};
    const auto rooms = m_workspace.m_missions.missions();
    for (const auto& value : rooms) {
        if (value.toMap().value(QStringLiteral("id")).toString() == reference)
            return reference;
    }
    for (const auto& value : rooms) {
        const auto room = value.toMap();
        if (room.value(QStringLiteral("name")).toString().compare(reference, Qt::CaseInsensitive) == 0)
            return room.value(QStringLiteral("id")).toString();
    }
    return {};
}

QString DosiModel::moodLabel(const QString& name)
{
    return DosiWords::mood(name);
}

void DosiModel::apply(const QString& type, const QJsonObject& event)
{
    if (type == QStringLiteral("dosi.snapshot"))
        applySnapshot(event.value(QStringLiteral("dosi")).toObject());
    else if (type == QStringLiteral("dosi.item"))
        upsert(event.value(QStringLiteral("item")).toObject().toVariantMap());
    else if (type == QStringLiteral("dosi.result"))
        receiveResult(event);
    else if (type == QStringLiteral("dosi.error"))
        receiveError(event);
}

void DosiModel::applySnapshot(const QJsonObject& dosi)
{
    const auto conversation = [](const QJsonObject& snapshot) {
        return snapshot.value(QStringLiteral("conversation")).toObject().value(QStringLiteral("id")).toString();
    };
    const bool replaced = m_snapshot.isEmpty() || conversation(m_snapshot) != conversation(dosi);
    m_snapshot = dosi;
    const auto items = dosi.value(QStringLiteral("items")).toArray().toVariantList();
    qsizetype start = -1;
    if (!replaced && !items.isEmpty()) {
        const auto first = idOf(items.first());
        for (qsizetype index = 0; index < m_items.size(); ++index) {
            if (idOf(m_items.at(index)) == first) {
                start = index;
                break;
            }
        }
    }
    if (start < 0) {
        m_items = items;
        m_earlier = dosi.value(QStringLiteral("earlier")).toBool();
    } else {
        m_items = m_items.mid(0, start) + items;
    }
    if (!m_primed && !conversation(dosi).isEmpty()) {
        m_primed = true;
        if (!busy() && !m_items.isEmpty())
            m_seenTurn = turnOf(m_items.last());
    }
    rebuild();
}

void DosiModel::upsert(const QVariantMap& item)
{
    const auto id = idOf(item);
    const auto found = std::find_if(m_items.begin(), m_items.end(), [&](const QVariant& entry) { return idOf(entry) == id; });
    if (found != m_items.end())
        *found = item;
    else
        m_items.append(item);
    touch(item);
    rebuild();
}

void DosiModel::touch(const QVariantMap& item)
{
    const auto content = bodyOf(item);
    const auto target = content.value(QStringLiteral("target")).toMap();
    if (content.value(QStringLiteral("kind")).toString() != QStringLiteral("tool")
        || target.value(QStringLiteral("kind")).toString() != QStringLiteral("terminal"))
        return;
    const auto session = terminalId(target.value(QStringLiteral("value")).toString());
    if (session.isEmpty())
        return;
    const auto id = idOf(item);
    auto current = m_touches.value(session).toMap();
    if (!m_touched.contains(id)) {
        if (m_touched.size() >= 512)
            m_touched.clear();
        m_touched.insert(id);
        current = { { QStringLiteral("count"), current.value(QStringLiteral("count")).toInt() + 1 }, { QStringLiteral("item"), id } };
    } else if (current.value(QStringLiteral("item")).toString() != id) {
        return;
    }
    static const QStringList shown { QStringLiteral("type"), QStringLiteral("press"), QStringLiteral("interrupt") };
    current.insert(QStringLiteral("said"), DosiWords::said(content));
    current.insert(QStringLiteral("running"), content.value(QStringLiteral("status")).toString() == QStringLiteral("running"));
    current.insert(QStringLiteral("shows"), shown.contains(content.value(QStringLiteral("action")).toString()));
    m_touches.insert(session, current);
    emit touchesChanged();
}

void DosiModel::receiveResult(const QJsonObject& event)
{
    const auto found = m_pending.constFind(event.value(QStringLiteral("requestId")).toString());
    if (found == m_pending.cend())
        return;
    const auto pending = found.value();
    m_pending.erase(found);
    if (pending.operation.startsWith(QStringLiteral("dosi.instructions"))) {
        const auto saved = event.value(QStringLiteral("instructions")).toObject();
        m_instructions = { { QStringLiteral("text"), saved.value(QStringLiteral("text")).toString() },
            { QStringLiteral("edited"), saved.value(QStringLiteral("edited")).toBool() },
            { QStringLiteral("loaded"), true }, { QStringLiteral("saving"), false }, { QStringLiteral("message"), QString {} } };
        emit instructionsChanged();
        return;
    }
    if (pending.operation == QStringLiteral("dosi.setup")) {
        emit saved();
        return;
    }
    if (pending.operation != QStringLiteral("dosi.history"))
        return;
    m_loadingEarlier = false;
    const auto page = event.value(QStringLiteral("page")).toObject();
    if (!m_items.isEmpty() && idOf(m_items.first()) == pending.value) {
        QSet<QString> known;
        for (const auto& item : std::as_const(m_items))
            known.insert(idOf(item));
        QVariantList earlier;
        for (const auto& value : page.value(QStringLiteral("items")).toArray()) {
            const auto item = value.toObject().toVariantMap();
            if (!known.contains(idOf(item)))
                earlier.append(item);
        }
        m_items = earlier + m_items;
        m_earlier = page.value(QStringLiteral("earlier")).toBool();
    }
    rebuild();
}

void DosiModel::receiveError(const QJsonObject& event)
{
    const auto message = event.value(QStringLiteral("message")).toString();
    const auto pending = m_pending.take(event.value(QStringLiteral("requestId")).toString());
    if (pending.operation.startsWith(QStringLiteral("dosi.instructions"))) {
        m_instructions.insert(QStringLiteral("saving"), false);
        m_instructions.insert(QStringLiteral("message"), message);
        emit instructionsChanged();
        return;
    }
    if (pending.operation == QStringLiteral("dosi.history")) {
        m_loadingEarlier = false;
        emit changed();
    }
    if (pending.operation == QStringLiteral("dosi.send") && m_draft.isEmpty())
        setDraft(pending.value);
    m_workspace.setError(message);
}

void DosiModel::request(const QString& type, QJsonObject values, std::optional<QString> tracked)
{
    const auto id = m_workspace.sendRequest(type, std::move(values));
    if (id.isEmpty()) {
        if (type == QStringLiteral("dosi.send") && tracked && m_draft.isEmpty())
            setDraft(*tracked);
        return;
    }
    if (tracked)
        m_pending.insert(id, { type, *tracked });
}

QVariantMap DosiModel::present(const QVariantMap& tool, const QString& itemId) const
{
    const auto target = tool.value(QStringLiteral("target")).toMap();
    const auto targetKind = target.value(QStringLiteral("kind")).toString();
    const auto targetValue = target.value(QStringLiteral("value")).toString();
    const auto action = tool.value(QStringLiteral("action")).toString();
    const auto progress = tool.value(QStringLiteral("status")).toString();
    static const QStringList openable { QStringLiteral("terminal"), QStringLiteral("room"), QStringLiteral("rooms"),
        QStringLiteral("terminals"), QStringLiteral("url") };
    QVariantMap shown { { QStringLiteral("item"), itemId }, { QStringLiteral("action"), action },
        { QStringLiteral("status"), progress }, { QStringLiteral("running"), progress == QStringLiteral("running") },
        { QStringLiteral("text"), tool.value(QStringLiteral("text")).toString() },
        { QStringLiteral("output"), tool.value(QStringLiteral("output")).toString() },
        { QStringLiteral("hasOutput"), !tool.value(QStringLiteral("output")).isNull() },
        { QStringLiteral("targetKind"), targetKind }, { QStringLiteral("targetValue"), targetValue },
        { QStringLiteral("receipt"), DosiWords::receipt(tool) }, { QStringLiteral("icon"), DosiWords::deed(action).icon },
        { QStringLiteral("openable"), openable.contains(targetKind) } };
    if (targetKind == QStringLiteral("terminal"))
        shown.insert(QStringLiteral("sessionId"), terminalId(targetValue));
    if (targetKind == QStringLiteral("room"))
        shown.insert(QStringLiteral("roomId"), roomId(targetValue));
    return shown;
}

QVariantMap DosiModel::row(const QVariantMap& item, const QVariantList& run, bool starts) const
{
    const auto id = idOf(item);
    const auto content = bodyOf(item);
    const auto type = content.value(QStringLiteral("kind")).toString();
    QVariantMap shown { { QStringLiteral("id"), id }, { QStringLiteral("turn"), turnOf(item) },
        { QStringLiteral("starts"), starts }, { QStringLiteral("kind"), type } };
    if (run.size() > 1) {
        QVariantList tools;
        QVariantList bodies;
        QVariantList marks;
        QStringList targets;
        bool active = false;
        bool failed = false;
        for (const auto& entry : run) {
            const auto tool = bodyOf(entry);
            const auto presented = present(tool, idOf(entry));
            bodies.append(tool);
            tools.append(presented);
            active = active || presented.value(QStringLiteral("running")).toBool();
            failed = failed || presented.value(QStringLiteral("status")).toString() == QStringLiteral("failed");
            const auto key = presented.value(QStringLiteral("targetKind")).toString() + QLatin1Char('\n')
                + presented.value(QStringLiteral("targetValue")).toString();
            if (!targets.contains(key) && marks.size() < 5) {
                targets.append(key);
                marks.append(presented);
            }
        }
        shown.insert(QStringLiteral("kind"), QStringLiteral("tools"));
        shown.insert(QStringLiteral("tools"), tools);
        shown.insert(QStringLiteral("marks"), marks);
        shown.insert(QStringLiteral("summary"), DosiWords::summary(bodies));
        shown.insert(QStringLiteral("running"), active);
        shown.insert(QStringLiteral("failed"), failed);
        return shown;
    }
    if (type == QStringLiteral("tool")) {
        shown.insert(QStringLiteral("tool"), present(content, id));
        return shown;
    }
    if (type == QStringLiteral("question")) {
        const auto reply = content.value(QStringLiteral("answer"));
        const auto reference = content.value(QStringLiteral("terminal")).toString();
        const auto session = terminalId(reference);
        const auto options = content.value(QStringLiteral("options")).toStringList();
        const bool multiple = content.value(QStringLiteral("multiple")).toBool();
        shown.insert(QStringLiteral("question"), content.value(QStringLiteral("question")).toString());
        shown.insert(QStringLiteral("options"), options);
        shown.insert(QStringLiteral("multiple"), multiple);
        shown.insert(QStringLiteral("sessionId"), session);
        shown.insert(QStringLiteral("open"), reply.isNull());
        if (!reply.isNull()) {
            const auto chosen = reply.toStringList();
            auto line = chosen.isEmpty() ? tr("not answered") : chosen.join(QStringLiteral(", "));
            if (!reference.isEmpty()) {
                const auto name = m_sessions.presentationForSession(session).value(QStringLiteral("name")).toString();
                line += QStringLiteral(" · ") + (name.isEmpty() ? reference : name);
            }
            shown.insert(QStringLiteral("answered"), line);
            shown.insert(QStringLiteral("skipped"), chosen.isEmpty());
        }
        return shown;
    }
    shown.insert(QStringLiteral("text"), content.value(QStringLiteral("text")).toString());
    shown.insert(QStringLiteral("live"), content.value(QStringLiteral("live")).toBool());
    if (type == QStringLiteral("reasoning"))
        shown.insert(QStringLiteral("seconds"),
            content.value(QStringLiteral("seconds")).isNull() ? -1 : content.value(QStringLiteral("seconds")).toInt());
    if (type == QStringLiteral("notice"))
        shown.insert(QStringLiteral("tone"), content.value(QStringLiteral("tone")).toString());
    return shown;
}

QVariantList DosiModel::buildRows(const QDateTime& now) const
{
    QVariantList result;
    QVariant previous;
    QString shown;
    for (qsizetype index = 0; index < m_items.size(); ++index) {
        const auto item = m_items.at(index).toMap();
        const auto at = QDateTime::fromMSecsSinceEpoch(item.value(QStringLiteral("at")).toLongLong());
        const auto before = previous.isValid()
            ? QDateTime::fromMSecsSinceEpoch(previous.toMap().value(QStringLiteral("at")).toLongLong())
            : QDateTime {};
        if (!previous.isValid() || before.date() != at.date() || before.msecsTo(at) >= 3600 * 1000) {
            const auto label = DosiWords::day(at, now);
            if (label != shown)
                result.append(QVariantMap { { QStringLiteral("id"), QStringLiteral("divider.") + idOf(item) },
                    { QStringLiteral("kind"), QStringLiteral("divider") }, { QStringLiteral("label"), label } });
            shown = label;
        }
        const bool starts = !previous.isValid() || kindOf(previous) == QStringLiteral("person") || turnOf(previous) != turnOf(item);
        QVariantList run { item };
        if (kindOf(item) == QStringLiteral("tool")) {
            while (index + 1 < m_items.size() && kindOf(m_items.at(index + 1)) == QStringLiteral("tool")
                && turnOf(m_items.at(index + 1)) == turnOf(item))
                run.append(m_items.at(++index));
        }
        result.append(row(item, run, starts));
        previous = run.last();
    }
    return result;
}

void DosiModel::rebuild()
{
    m_rows = buildRows(QDateTime::currentDateTime());
    m_terminalQuestions.clear();
    for (const auto& item : std::as_const(m_items)) {
        const auto content = bodyOf(item);
        if (!openQuestion(content))
            continue;
        const auto session = terminalId(content.value(QStringLiteral("terminal")).toString());
        if (session.isEmpty())
            continue;
        m_terminalQuestions.insert(session, QVariantMap { { QStringLiteral("item"), idOf(item) },
            { QStringLiteral("question"), content.value(QStringLiteral("question")).toString() },
            { QStringLiteral("options"), content.value(QStringLiteral("options")).toStringList() },
            { QStringLiteral("multiple"), content.value(QStringLiteral("multiple")).toBool() } });
    }
    const auto& desktop = m_workspace.m_desktop;
    if (desktop.dosiOpen() && desktop.sidebarOpen() && !m_items.isEmpty())
        m_seenTurn = turnOf(m_items.last());
    emit changed();
}

void DosiModel::refreshSeen()
{
    const auto& desktop = m_workspace.m_desktop;
    if (!desktop.dosiOpen() || !desktop.sidebarOpen() || m_items.isEmpty() || m_seenTurn == turnOf(m_items.last()))
        return;
    m_seenTurn = turnOf(m_items.last());
    emit changed();
}

void DosiModel::reset()
{
    m_snapshot = {};
    m_items.clear();
    m_rows.clear();
    m_terminalQuestions.clear();
    m_touches.clear();
    m_touched.clear();
    m_instructions.clear();
    m_pending.clear();
    m_seenTurn.clear();
    m_primed = false;
    m_earlier = false;
    m_loadingEarlier = false;
    setDraft({});
    emit changed();
    emit touchesChanged();
    emit instructionsChanged();
}

}
