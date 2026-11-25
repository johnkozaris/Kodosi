#include "presentation/TerminalChat.hpp"
#include "presentation/AgentLauncherModel.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/ChatProse.hpp"
#include "presentation/ChatWords.hpp"
#include "presentation/Link.hpp"
#include "presentation/SessionCatalogModel.hpp"

#include <QDateTime>
#include <QDebug>
#include <QDesktopServices>
#include <QDir>
#include <QJsonDocument>
#include <QLocale>
#include <QSet>

#include <algorithm>
#include <cmath>

namespace kodosi {
namespace {
const QString chatKey = QStringLiteral("chat");
constexpr qint64 longTurn = 30000;
constexpr qsizetype longCommand = 96;
constexpr qsizetype mostChangeLines = 40;
constexpr qsizetype mostAskLines = 14;

QString text(const QJsonObject& object, const char* key)
{
    return object.value(QLatin1StringView(key)).toString();
}

QJsonObject body(const QJsonObject& item)
{
    return item.value(QStringLiteral("body")).toObject();
}

QString kindOf(const QJsonObject& item)
{
    return text(body(item), "kind");
}

QJsonObject toolOf(const QJsonObject& item)
{
    return body(item).value(QStringLiteral("tool")).toObject();
}

qint64 at(const QJsonObject& item)
{
    return static_cast<qint64>(item.value(QStringLiteral("at")).toDouble());
}

bool open(const QJsonObject& tool)
{
    const auto state = text(tool, "state");
    return state == QStringLiteral("preparing") || state == QStringLiteral("asking") || state == QStringLiteral("running");
}

QString detailKind(const QJsonObject& tool)
{
    return text(tool.value(QStringLiteral("detail")).toObject(), "kind");
}

bool answers(const QJsonObject& item)
{
    const auto kind = kindOf(item);
    if (kind == QStringLiteral("text"))
        return true;
    const auto tool = toolOf(item);
    return kind == QStringLiteral("tool") && detailKind(tool) == QStringLiteral("plan") && text(tool, "state") != QStringLiteral("asking");
}

QList<QJsonObject> toolsOf(const QList<QJsonObject>& items)
{
    QList<QJsonObject> tools;
    for (const auto& item : items) {
        if (kindOf(item) == QStringLiteral("tool"))
            tools.append(toolOf(item));
    }
    return tools;
}

// The chat as a person reads it: what the agent writes during a turn shows as the agent's words, and
// the steps between them fold into one row each.
struct Row {
    QString id;
    QString kind;
    qint64 at = 0;
    QJsonObject item;
    QList<QJsonObject> work;
    QString text;
    bool live = false;

    [[nodiscard]] bool agents() const
    {
        if (kind == QStringLiteral("person"))
            return false;
        if (kind == QStringLiteral("note"))
            return text != QStringLiteral("compacted");
        return true;
    }
};

std::optional<Row> answerRow(const QJsonObject& item, bool ended)
{
    const auto id = text(item, "id");
    const auto values = body(item);
    if (kindOf(item) == QStringLiteral("text"))
        return Row { .id = id, .kind = QStringLiteral("answer"), .at = at(item), .item = {}, .work = {},
            .text = text(values, "text"), .live = values.value(QStringLiteral("live")).toBool() && !ended };
    const auto tool = toolOf(item);
    const auto plan = text(tool.value(QStringLiteral("detail")).toObject(), "text");
    if (detailKind(tool) != QStringLiteral("plan") || plan.isEmpty())
        return std::nullopt;
    return Row { .id = id, .kind = QStringLiteral("plan"), .at = at(item), .item = {}, .work = {}, .text = plan,
        .live = open(tool) && !ended };
}

QList<Row> rows(const QList<QJsonObject>& items, bool lastTurnRuns)
{
    QList<Row> rows;
    QList<QJsonObject> turn;
    const auto fold = [&](bool ended) {
        if (turn.isEmpty())
            return;
        std::optional<qsizetype> answer;
        if (ended) {
            for (auto index = turn.size() - 1; index >= 0; --index) {
                if (answers(turn.at(index))) {
                    answer = index;
                    break;
                }
            }
        } else if (answers(turn.last())) {
            answer = turn.size() - 1;
        }
        QList<QJsonObject> work;
        const auto endWork = [&] {
            if (work.isEmpty())
                return;
            rows.append(Row { .id = QStringLiteral("work.") + text(work.first(), "id"), .kind = QStringLiteral("work"),
                .at = at(work.first()), .item = {}, .work = work, .text = {}, .live = false });
            work.clear();
        };
        for (qsizetype index = 0; index < turn.size(); ++index) {
            const auto& item = turn.at(index);
            if (answer == index) {
                endWork();
                if (auto row = answerRow(item, ended))
                    rows.append(*row);
            } else if (kindOf(item) == QStringLiteral("text")) {
                endWork();
                rows.append(Row { .id = text(item, "id"), .kind = QStringLiteral("answer"), .at = at(item), .item = {},
                    .work = {}, .text = text(body(item), "text"), .live = false });
            } else {
                work.append(item);
            }
        }
        endWork();
        turn.clear();
    };
    for (const auto& item : items) {
        const auto kind = kindOf(item);
        if (kind == QStringLiteral("person")) {
            fold(true);
            rows.append(Row { .id = text(item, "id"), .kind = kind, .at = at(item), .item = item, .work = {}, .text = {}, .live = false });
        } else if (kind == QStringLiteral("text") || kind == QStringLiteral("tool")) {
            turn.append(item);
        } else if (kind == QStringLiteral("turn")) {
            fold(true);
            const auto end = text(body(item), "end");
            const auto ms = static_cast<qint64>(body(item).value(QStringLiteral("ms")).toDouble());
            if (end != QStringLiteral("done") || ms >= longTurn)
                rows.append(Row { .id = text(item, "id"), .kind = QStringLiteral("end"), .at = at(item), .item = item,
                    .work = {}, .text = end, .live = false });
        } else if (kind == QStringLiteral("note")) {
            fold(true);
            rows.append(Row { .id = text(item, "id"), .kind = kind, .at = at(item), .item = item, .work = {},
                .text = text(body(item).value(QStringLiteral("note")).toObject(), "kind"), .live = false });
        }
    }
    fold(!lastTurnRuns);
    return rows;
}

QString mainKind(const QList<QJsonObject>& tools)
{
    QHash<QString, int> counts;
    for (const auto& tool : tools)
        ++counts[text(tool, "kind")];
    QString main = QStringLiteral("other");
    int most = 0;
    for (const auto& tool : tools) {
        const auto kind = text(tool, "kind");
        if (counts.value(kind) > most) {
            most = counts.value(kind);
            main = kind;
        }
    }
    return main;
}

bool mono(const QString& kind)
{
    static const QSet<QString> code { QStringLiteral("command"), QStringLiteral("read"), QStringLiteral("edit"),
        QStringLiteral("write"), QStringLiteral("search") };
    return code.contains(kind);
}

QVariantList changeLines(const QJsonArray& lines, qsizetype most)
{
    QVariantList shaped;
    for (qsizetype index = 0; index < std::min(lines.size(), most); ++index) {
        const auto line = lines.at(index).toObject();
        shaped.append(QVariantMap { { QStringLiteral("sign"), text(line, "sign") }, { QStringLiteral("text"), text(line, "text") } });
    }
    return shaped;
}

std::pair<QString, bool> offer(const QJsonObject& always)
{
    const auto kind = text(always, "kind");
    if (kind == QStringLiteral("rule"))
        return { text(always, "text"), true };
    if (kind == QStringLiteral("folder"))
        return { text(always, "name"), true };
    if (kind == QStringLiteral("mode"))
        return { ChatWords::mode(text(always, "mode")), false };
    return {};
}

QString planLabel(const QString& choice)
{
    if (choice == QStringLiteral("go"))
        return TerminalChat::tr("Approve");
    if (choice == QStringLiteral("ask"))
        return TerminalChat::tr("Approve, and ask before each change");
    if (choice == QStringLiteral("edits"))
        return TerminalChat::tr("Approve, and allow its edits");
    if (choice == QStringLiteral("auto"))
        return TerminalChat::tr("Approve, and use the safety check");
    if (choice == QStringLiteral("alone"))
        return TerminalChat::tr("Approve, and ask nothing");
    if (choice == QStringLiteral("helpers"))
        return TerminalChat::tr("Approve, and let helpers work alone");
    if (choice == QStringLiteral("leave"))
        return TerminalChat::tr("Leave plan mode");
    return {};
}

QString answerKey(const QJsonObject& answer)
{
    const auto kind = text(answer, "kind");
    if (kind == QStringLiteral("plan"))
        return QStringLiteral("plan.") + text(answer, "choice");
    if (kind == QStringLiteral("deny") && answer.contains(QStringLiteral("text")))
        return QStringLiteral("words");
    return kind;
}

bool knownItem(const QJsonValue& value)
{
    static const QSet<QString> kinds { QStringLiteral("person"), QStringLiteral("text"), QStringLiteral("tool"),
        QStringLiteral("turn"), QStringLiteral("note") };
    const auto item = value.toObject();
    return item.value(QStringLiteral("id")).isString() && item.value(QStringLiteral("at")).isDouble()
        && kinds.contains(kindOf(item));
}

bool knownState(const QJsonValue& value)
{
    static const QSet<QString> states { QStringLiteral("away"), QStringLiteral("idle"), QStringLiteral("working"),
        QStringLiteral("asking") };
    return states.contains(value.toString());
}
}

TerminalChat::TerminalChat(CanvasDocuments& documents, QString sessionId, QObject* parent)
    : QObject(parent)
    , m_documents(&documents)
    , m_sessionId(std::move(sessionId))
{
}

TerminalChat::~TerminalChat()
{
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
}

QString TerminalChat::state() const
{
    return m_chat ? m_chat->state : QStringLiteral("away");
}

bool TerminalChat::working() const
{
    const auto now = state();
    return now == QStringLiteral("working") || now == QStringLiteral("asking");
}

bool TerminalChat::canWrite() const
{
    return m_linked && state() != QStringLiteral("away");
}

bool TerminalChat::asking() const
{
    return !question().isEmpty();
}

QString TerminalChat::question() const
{
    if (!m_chat)
        return {};
    for (const auto& ask : m_chat->asks) {
        const auto id = text(ask.toObject(), "id");
        if (!m_answers.contains(id))
            return id;
    }
    return {};
}

QVariantMap TerminalChat::answers() const
{
    QVariantMap answers;
    for (const auto& [ask, answer] : m_answers.asKeyValueRange())
        answers.insert(ask, QVariantMap { { QStringLiteral("key"), answerKey(answer) },
                                { QStringLiteral("answers"), answer.value(QStringLiteral("answers")).toVariant() } });
    return answers;
}

void TerminalChat::setDraft(const QString& draft)
{
    if (m_draft == draft)
        return;
    m_draft = draft;
    emit draftChanged();
}

void TerminalChat::watch()
{
    ++m_watchers;
    if (!m_pipe.isEmpty() || !m_documents)
        return;
    m_awaitsSnapshot = true;
    m_pipe = m_documents->open(m_sessionId, chatKey);
}

void TerminalChat::unwatch()
{
    if (m_watchers == 0 || --m_watchers > 0)
        return;
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_pipe.clear();
    setLinked(false);
}

bool TerminalChat::submit()
{
    const auto words = m_draft.trimmed();
    if (words.isEmpty() || !canWrite())
        return false;
    setDraft({});
    if (const auto waiting = question(); !waiting.isEmpty())
        answer(waiting, { { QStringLiteral("kind"), QStringLiteral("deny") }, { QStringLiteral("text"), words } });
    else
        request({ { QStringLiteral("do"), QStringLiteral("send") }, { QStringLiteral("text"), words } });
    return true;
}

void TerminalChat::answer(const QString& ask, const QVariantMap& answer)
{
    if (m_answers.contains(ask) || !m_linked)
        return;
    const auto kind = answer.value(QStringLiteral("kind")).toString();
    QJsonObject given { { QStringLiteral("kind"), kind } };
    if (kind == QStringLiteral("deny") && !answer.value(QStringLiteral("text")).toString().isEmpty())
        given.insert(QStringLiteral("text"), answer.value(QStringLiteral("text")).toString());
    else if (kind == QStringLiteral("answers"))
        given.insert(QStringLiteral("answers"), QJsonArray::fromVariantList(answer.value(QStringLiteral("answers")).toList()));
    else if (kind == QStringLiteral("plan"))
        given.insert(QStringLiteral("choice"), answer.value(QStringLiteral("choice")).toString());
    else if (kind != QStringLiteral("allow") && kind != QStringLiteral("always") && kind != QStringLiteral("deny"))
        return;
    m_answers.insert(ask, given);
    request({ { QStringLiteral("do"), QStringLiteral("answer") }, { QStringLiteral("ask"), ask }, { QStringLiteral("answer"), given } });
    emit asksChanged();
}

void TerminalChat::unqueue(const QString& id)
{
    request({ { QStringLiteral("do"), QStringLiteral("unqueue") }, { QStringLiteral("id"), id } });
}

void TerminalChat::stop()
{
    request({ { QStringLiteral("do"), QStringLiteral("stop") } });
}

void TerminalChat::toggle(const QString& id)
{
    if (!m_opened.removeOne(id))
        m_opened.append(id);
    emit openedChanged();
}

void TerminalChat::request(const QJsonObject& request)
{
    if (!m_documents || m_pipe.isEmpty())
        return;
    m_documents->send(m_pipe, QString::fromLatin1(QJsonDocument(request).toJson(QJsonDocument::Compact).toBase64()));
}

void TerminalChat::pipeOpened()
{
    m_awaitsSnapshot = true;
}

void TerminalChat::lose(const QString& reason)
{
    m_failure = reason;
    setLinked(false);
    emit stateChanged();
}

void TerminalChat::pipeClosed()
{
    m_pipe.clear();
    setLinked(false);
}

void TerminalChat::setLinked(bool linked)
{
    if (m_linked == linked)
        return;
    m_linked = linked;
    emit stateChanged();
}

// Reopen the pipe to fetch a fresh snapshot from the host after a missed or unreadable update.
void TerminalChat::renew()
{
    if (!m_documents || m_pipe.isEmpty())
        return;
    m_awaitsSnapshot = true;
    setLinked(false);
    m_documents->renew(m_pipe);
}

std::optional<TerminalChat::Snapshot> TerminalChat::snapshotFrom(const QJsonObject& value)
{
    const auto items = value.value(QStringLiteral("items"));
    if (!value.value(QStringLiteral("revision")).isDouble() || !knownState(value.value(QStringLiteral("state")))
        || !value.value(QStringLiteral("facts")).isObject() || !items.isArray())
        return std::nullopt;
    Snapshot chat;
    chat.revision = static_cast<quint64>(value.value(QStringLiteral("revision")).toDouble());
    chat.state = value.value(QStringLiteral("state")).toString();
    chat.now = value.value(QStringLiteral("now")).toObject();
    chat.facts = value.value(QStringLiteral("facts")).toObject();
    for (const auto& item : items.toArray()) {
        if (!knownItem(item))
            return std::nullopt;
        chat.items.append(item.toObject());
    }
    chat.asks = value.value(QStringLiteral("asks")).toArray();
    chat.queued = value.value(QStringLiteral("queued")).toArray();
    chat.tasks = value.value(QStringLiteral("tasks")).toArray();
    chat.commands = value.value(QStringLiteral("commands")).toArray();
    return chat;
}

bool TerminalChat::apply(Snapshot& chat, const QJsonObject& change)
{
    const auto kind = change.value(QStringLiteral("change")).toString();
    const auto lastOf = [&chat](const QString& id) {
        for (auto index = chat.items.size() - 1; index >= 0; --index) {
            if (text(chat.items.at(index), "id") == id)
                return index;
        }
        return qsizetype(-1);
    };
    if (kind == QStringLiteral("item")) {
        const auto item = change.value(QStringLiteral("item"));
        if (!knownItem(item))
            return false;
        const auto found = lastOf(text(item.toObject(), "id"));
        if (found >= 0)
            chat.items[found] = item.toObject();
        else
            chat.items.append(item.toObject());
    } else if (kind == QStringLiteral("text")) {
        const auto found = lastOf(text(change, "id"));
        if (found >= 0 && kindOf(chat.items.at(found)) == QStringLiteral("text")) {
            auto& item = chat.items[found];
            auto values = body(item);
            values.insert(QStringLiteral("text"), text(values, "text") + text(change, "text"));
            item.insert(QStringLiteral("body"), values);
        }
    } else if (kind == QStringLiteral("remove")) {
        const auto id = text(change, "id");
        chat.items.removeIf([&id](const QJsonObject& item) { return text(item, "id") == id; });
    } else if (kind == QStringLiteral("asks")) {
        chat.asks = change.value(QStringLiteral("asks")).toArray();
    } else if (kind == QStringLiteral("queued")) {
        chat.queued = change.value(QStringLiteral("queued")).toArray();
    } else if (kind == QStringLiteral("tasks")) {
        chat.tasks = change.value(QStringLiteral("tasks")).toArray();
    } else if (kind == QStringLiteral("commands")) {
        chat.commands = change.value(QStringLiteral("commands")).toArray();
    } else if (kind == QStringLiteral("facts")) {
        chat.facts = change.value(QStringLiteral("facts")).toObject();
    } else if (kind == QStringLiteral("state")) {
        if (!knownState(change.value(QStringLiteral("state"))))
            return false;
        chat.state = change.value(QStringLiteral("state")).toString();
        chat.now = change.value(QStringLiteral("now")).toObject();
    } else if (kind == QStringLiteral("reset")) {
        chat.items.clear();
        chat.asks = {};
        chat.tasks = {};
    } else {
        return false;
    }
    return true;
}

void TerminalChat::receive(const QByteArray& frame)
{
    const auto value = QJsonDocument::fromJson(frame).object();
    const auto unreadable = [this] {
        qWarning().noquote() << "A chat frame could not be read";
        if (!m_awaitsSnapshot)
            renew();
    };
    if (value.contains(QStringLiteral("snapshot"))) {
        auto chat = snapshotFrom(value.value(QStringLiteral("snapshot")).toObject());
        if (!chat) {
            unreadable();
            return;
        }
        m_awaitsSnapshot = false;
        m_chat = std::move(chat);
        // The host removes an ask once it processes the answer, and sends during a broken pipe are
        // lost, so any ask still present in a fresh snapshot has not been answered yet.
        m_answers.clear();
        m_failure.clear();
        m_linked = true;
        shape();
        emit threadChanged();
        emit stateChanged();
        emit asksChanged();
        return;
    }
    const auto update = value.value(QStringLiteral("update")).toObject();
    const auto changes = update.value(QStringLiteral("changes"));
    if (!update.value(QStringLiteral("revision")).isDouble() || !changes.isArray()) {
        unreadable();
        return;
    }
    const auto revision = static_cast<quint64>(update.value(QStringLiteral("revision")).toDouble());
    if (m_awaitsSnapshot || !m_chat || revision <= m_chat->revision)
        return;
    if (revision != m_chat->revision + 1) {
        renew();
        return;
    }
    auto chat = *m_chat;
    for (const auto& change : changes.toArray()) {
        if (!apply(chat, change.toObject())) {
            unreadable();
            return;
        }
    }
    chat.revision = revision;
    const bool stateMoved = chat.state != m_chat->state;
    m_chat = std::move(chat);
    const auto before = m_answers.size();
    m_answers.removeIf([this](const auto& answered) {
        return std::ranges::none_of(m_chat->asks, [&answered](const QJsonValue& ask) {
            return text(ask.toObject(), "id") == answered.key();
        });
    });
    if (m_answers.size() != before)
        emit asksChanged();
    shape();
    if (stateMoved)
        emit stateChanged();
}

QVariantList TerminalChat::blocks(const QString& id, const QString& text)
{
    const auto folder = kodosi::text(m_chat->facts, "folder");
    auto& kept = m_prose[id];
    if (kept.text != text || kept.folder != folder || kept.blocks.isEmpty())
        kept = Prose { .text = text, .folder = folder, .blocks = ChatProse::blocks(text, folder) };
    return kept.blocks;
}

QString TerminalChat::nowWords() const
{
    const auto kind = text(m_chat->now, "kind");
    if (kind == QStringLiteral("writing"))
        return tr("writing");
    if (kind == QStringLiteral("compacting"))
        return tr("compacting");
    if (kind == QStringLiteral("tool")) {
        const auto id = text(m_chat->now, "item");
        for (auto index = m_chat->items.size() - 1; index >= 0; --index) {
            const auto& item = m_chat->items.at(index);
            if (text(item, "id") == id && kindOf(item) == QStringLiteral("tool"))
                return ChatWords::step(toolOf(item));
        }
    }
    return tr("thinking");
}

QVariantMap TerminalChat::step(const QJsonObject& item)
{
    const auto id = text(item, "id");
    const auto tool = toolOf(item);
    const auto kind = text(tool, "kind");
    const auto target = text(tool, "target");
    const auto detail = tool.value(QStringLiteral("detail")).toObject();
    const auto shape = detailKind(tool);
    QVariantMap shown { { QStringLiteral("kind"), shape } };
    bool opens = false;
    if (shape == QStringLiteral("command")) {
        const auto command = text(detail, "command");
        const auto output = text(detail, "output");
        const auto background = detail.value(QStringLiteral("background")).toBool();
        shown.insert({ { QStringLiteral("command"), command }, { QStringLiteral("output"), output },
            { QStringLiteral("background"), background },
            { QStringLiteral("code"), command != target || !output.isEmpty() } });
        opens = !output.isEmpty() || background || command != target;
    } else if (shape == QStringLiteral("change")) {
        const auto path = text(detail, "path");
        const auto lines = detail.value(QStringLiteral("lines")).toArray();
        shown.insert({ { QStringLiteral("path"), path }, { QStringLiteral("name"), SessionCatalogModel::folderName(path) },
            { QStringLiteral("added"), detail.value(QStringLiteral("added")).toInt() },
            { QStringLiteral("removed"), detail.value(QStringLiteral("removed")).toInt() },
            { QStringLiteral("lines"), changeLines(lines, mostChangeLines) },
            { QStringLiteral("more"), lines.size() > mostChangeLines } });
        opens = true;
    } else if (shape == QStringLiteral("output")) {
        shown.insert(QStringLiteral("text"), text(detail, "text"));
        opens = !text(detail, "text").isEmpty();
    } else if (shape == QStringLiteral("question")) {
        const auto questions = detail.value(QStringLiteral("questions")).toArray();
        const auto given = detail.value(QStringLiteral("answers")).toArray();
        QVariantList pairs;
        for (qsizetype index = 0; index < questions.size(); ++index)
            pairs.append(QVariantMap { { QStringLiteral("question"), text(questions.at(index).toObject(), "question") },
                { QStringLiteral("answer"), index < given.size() ? given.at(index).toString() : QString {} } });
        shown.insert(QStringLiteral("pairs"), pairs);
        opens = !questions.isEmpty();
    } else if (shape == QStringLiteral("plan")) {
        shown.insert(QStringLiteral("blocks"), blocks(id + QStringLiteral(".plan"), text(detail, "text")));
        opens = true;
    } else if (shape == QStringLiteral("agent")) {
        const auto doing = detail.value(QStringLiteral("doing")).toObject();
        shown.insert({ { QStringLiteral("name"), text(detail, "name") },
            { QStringLiteral("steps"), detail.value(QStringLiteral("steps")).toInt() },
            { QStringLiteral("doing"), doing.isEmpty() ? QString {} : text(doing, "target") },
            { QStringLiteral("doingIcon"), doing.isEmpty() ? QString {} : ChatWords::icon(text(doing, "kind")) },
            { QStringLiteral("said"), text(detail, "said").isEmpty() ? QString {} : ChatProse::text(text(detail, "said")) } });
        opens = true;
    }
    return { { QStringLiteral("id"), id }, { QStringLiteral("tool"), kind },
        { QStringLiteral("icon"), ChatWords::icon(kind) }, { QStringLiteral("words"), ChatWords::step(tool) },
        { QStringLiteral("mono"), mono(kind) }, { QStringLiteral("open"), open(tool) },
        { QStringLiteral("state"), text(tool, "state") }, { QStringLiteral("name"), text(tool, "name") },
        { QStringLiteral("target"), target }, { QStringLiteral("opens"), opens }, { QStringLiteral("detail"), shown } };
}

QVariantMap TerminalChat::shapedAsk(const QJsonObject& ask)
{
    const auto values = ask.value(QStringLiteral("body")).toObject();
    const auto kind = text(values, "kind");
    QVariantMap shaped { { QStringLiteral("id"), text(ask, "id") }, { QStringLiteral("kind"), kind } };
    if (kind == QStringLiteral("permission")) {
        const auto tool = values.value(QStringLiteral("tool")).toObject();
        const auto target = text(tool, "target");
        const auto detail = tool.value(QStringLiteral("detail")).toObject();
        const auto command = detailKind(tool) == QStringLiteral("command") ? text(detail, "command") : QString {};
        const bool whole = !command.isEmpty() && (command != target || command.size() > longCommand);
        const auto [always, code] = offer(values.value(QStringLiteral("always")).toObject());
        shaped.insert({ { QStringLiteral("icon"), ChatWords::icon(text(tool, "kind")) },
            { QStringLiteral("name"), text(tool, "name") },
            { QStringLiteral("title"), target.isEmpty() ? text(tool, "name") : target },
            { QStringLiteral("whole"), whole ? command : QString {} },
            { QStringLiteral("lines"), whole || detailKind(tool) != QStringLiteral("change")
                    ? QVariantList {} : changeLines(detail.value(QStringLiteral("lines")).toArray(), mostAskLines) },
            { QStringLiteral("always"), always }, { QStringLiteral("alwaysCode"), code },
            { QStringLiteral("helper"), text(values, "helper") } });
    } else if (kind == QStringLiteral("question")) {
        const auto questions = values.value(QStringLiteral("questions")).toArray();
        QVariantList shapedQuestions;
        QStringList topics;
        for (const auto& value : questions) {
            const auto question = value.toObject();
            if (!text(question, "header").isEmpty())
                topics.append(text(question, "header"));
            QVariantList options;
            for (const auto& option : question.value(QStringLiteral("options")).toArray())
                options.append(QVariantMap { { QStringLiteral("label"), text(option.toObject(), "label") },
                    { QStringLiteral("description"), text(option.toObject(), "description") } });
            shapedQuestions.append(QVariantMap { { QStringLiteral("question"), text(question, "question") },
                { QStringLiteral("multiple"), question.value(QStringLiteral("multiple")).toBool() },
                { QStringLiteral("options"), options } });
        }
        shaped.insert({ { QStringLiteral("questions"), shapedQuestions },
            { QStringLiteral("topics"), topics.join(QStringLiteral(" · ")) },
            { QStringLiteral("direct"), questions.size() == 1
                    && !questions.first().toObject().value(QStringLiteral("multiple")).toBool() } });
    } else if (kind == QStringLiteral("plan")) {
        QVariantList choices;
        for (const auto& choice : values.value(QStringLiteral("choices")).toArray())
            choices.append(QVariantMap { { QStringLiteral("choice"), choice.toString() },
                { QStringLiteral("label"), planLabel(choice.toString()) } });
        shaped.insert({ { QStringLiteral("blocks"), blocks(text(ask, "id") + QStringLiteral(".ask"), text(values, "text")) },
            { QStringLiteral("choices"), choices } });
    }
    return shaped;
}

QVariantMap TerminalChat::shapedFacts() const
{
    const auto& facts = m_chat->facts;
    QVariantMap shaped { { QStringLiteral("agent"), text(facts, "agent") }, { QStringLiteral("model"), text(facts, "model") },
        { QStringLiteral("effort"), text(facts, "effort") },
        { QStringLiteral("background"), facts.value(QStringLiteral("background")).toInt() } };
    const auto mode = text(facts, "mode");
    shaped.insert({ { QStringLiteral("mode"), mode }, { QStringLiteral("modeLabel"), ChatWords::mode(mode) },
        { QStringLiteral("modeIcon"), ChatWords::modeIcon(mode) } });
    const auto context = facts.value(QStringLiteral("context")).toObject();
    if (!context.isEmpty()) {
        const auto window = context.value(QStringLiteral("window")).toDouble();
        const auto used = window <= 0 ? 0 : std::min(1.0, context.value(QStringLiteral("tokens")).toDouble() / window);
        shaped.insert({ { QStringLiteral("context"), static_cast<int>(std::lround(used * 100)) },
            { QStringLiteral("contextLevel"), used > 0.85 ? QStringLiteral("reached") : used >= 0.65 ? QStringLiteral("near") : QStringLiteral("normal") } });
    }
    if (!context.isEmpty()) {
        const QLocale locale;
        shaped.insert(QStringLiteral("tokens"), tr("%1 of %2 tokens")
                .arg(locale.toString(static_cast<qint64>(context.value(QStringLiteral("tokens")).toDouble())),
                    locale.toString(static_cast<qint64>(context.value(QStringLiteral("window")).toDouble()))));
    }
    QVariantList limits;
    for (const auto& limit : facts.value(QStringLiteral("limits")).toArray())
        limits.append(AgentLauncherModel::describedLimit(limit.toObject().toVariantMap()));
    shaped.insert(QStringLiteral("limits"), limits);
    shaped.insert(QStringLiteral("limit"), AgentLauncherModel::nearestLimit(limits, {}));
    const auto cost = facts.value(QStringLiteral("cost")).toDouble();
    shaped.insert(QStringLiteral("cost"), cost > 0 ? ChatWords::cost(cost) : QString {});
    return shaped;
}

void TerminalChat::shape()
{
    const auto live = working();
    const auto folded = rows(m_chat->items, live);
    QVariantList lines;
    std::optional<QDate> day;
    bool agentBefore = false;
    QSet<QString> kept;
    for (const auto& row : folded) {
        if (row.at > 0) {
            const auto date = QDateTime::fromMSecsSinceEpoch(row.at).date();
            if (day && *day != date) {
                lines.append(QVariantMap { { QStringLiteral("id"), QStringLiteral("day.") + row.id },
                    { QStringLiteral("kind"), QStringLiteral("day") }, { QStringLiteral("at"), row.at } });
                agentBefore = false;
            }
            day = date;
        }
        const bool agents = row.agents();
        QVariantMap line { { QStringLiteral("id"), row.id }, { QStringLiteral("kind"), row.kind },
            { QStringLiteral("at"), row.at }, { QStringLiteral("lead"), agents && !agentBefore } };
        agentBefore = agents;
        if (row.kind == QStringLiteral("person")) {
            const auto values = body(row.item);
            const auto by = values.value(QStringLiteral("by")).toObject();
            line.insert({ { QStringLiteral("html"), ChatProse::text(text(values, "text")) },
                { QStringLiteral("text"), text(values, "text") }, { QStringLiteral("byId"), text(by, "id") },
                { QStringLiteral("byName"), text(by, "name") } });
        } else if (row.kind == QStringLiteral("answer") || row.kind == QStringLiteral("plan")) {
            kept.insert(row.id);
            line.insert({ { QStringLiteral("blocks"), blocks(row.id, row.text) }, { QStringLiteral("live"), row.live },
                { QStringLiteral("text"), row.text } });
        } else if (row.kind == QStringLiteral("work")) {
            const auto tools = toolsOf(row.work);
            QList<QJsonObject> ended;
            QList<QJsonObject> done;
            for (const auto& tool : tools) {
                if (!open(tool))
                    ended.append(tool);
                if (text(tool, "state") == QStringLiteral("done"))
                    done.append(tool);
            }
            QVariantList steps;
            for (const auto& item : row.work) {
                if (kindOf(item) == QStringLiteral("tool") && detailKind(toolOf(item)) == QStringLiteral("plan"))
                    kept.insert(text(item, "id") + QStringLiteral(".plan"));
                steps.append(step(item));
            }
            line.insert({ { QStringLiteral("icon"), ChatWords::icon(mainKind(tools)) },
                { QStringLiteral("summary"), !done.isEmpty() ? ChatWords::summary(done)
                        : ended.isEmpty() ? tr("%n steps", "", static_cast<int>(tools.size())) : QString {} },
                { QStringLiteral("setbacks"), ChatWords::setbacks(ended) },
                { QStringLiteral("done"), !tools.isEmpty() && done.size() == tools.size() },
                { QStringLiteral("steps"), steps } });
        } else if (row.kind == QStringLiteral("end")) {
            const auto ms = static_cast<qint64>(body(row.item).value(QStringLiteral("ms")).toDouble());
            line.insert({ { QStringLiteral("end"), row.text },
                { QStringLiteral("worked"), ms >= longTurn ? tr("worked for %1").arg(ChatWords::duration(ms)) : QString {} } });
        } else if (row.kind == QStringLiteral("note")) {
            const auto note = body(row.item).value(QStringLiteral("note")).toObject();
            line.insert({ { QStringLiteral("note"), row.text }, { QStringLiteral("error"), text(note, "error") },
                { QStringLiteral("text"), text(note, "text") } });
        }
        lines.append(line);
    }
    const auto last = folded.isEmpty() ? QString {} : folded.last().kind;
    const bool answering = (last == QStringLiteral("answer") || last == QStringLiteral("plan")) && folded.last().live;
    if (live && m_chat->asks.isEmpty() && !answering)
        lines.append(QVariantMap { { QStringLiteral("id"), QStringLiteral("now") }, { QStringLiteral("kind"), QStringLiteral("now") },
            { QStringLiteral("lead"), !agentBefore }, { QStringLiteral("at"), 0 }, { QStringLiteral("words"), nowWords() } });
    if (m_chat->state == QStringLiteral("working")) {
        for (auto index = lines.size() - 1; index >= 0; --index) {
            auto line = lines.at(index).toMap();
            if (line.value(QStringLiteral("lead")).toBool()) {
                line.insert(QStringLiteral("working"), true);
                lines[index] = line;
                break;
            }
        }
    }
    const bool empty = folded.isEmpty() && !live;
    if (lines != m_lines || empty != m_empty) {
        m_lines = lines;
        m_empty = empty;
        emit threadChanged();
    }
    const auto ask = m_chat->asks.isEmpty() ? QVariantMap {} : shapedAsk(m_chat->asks.first().toObject());
    if (!ask.isEmpty())
        kept.insert(ask.value(QStringLiteral("id")).toString() + QStringLiteral(".ask"));
    m_prose.removeIf([&kept](const auto& entry) { return !kept.contains(entry.key()); });
    if (ask != m_ask) {
        m_ask = ask;
        emit asksChanged();
    }
    QVariantList queued;
    for (const auto& value : m_chat->queued) {
        const auto waiting = value.toObject();
        queued.append(QVariantMap { { QStringLiteral("id"), text(waiting, "id") }, { QStringLiteral("text"), text(waiting, "text") },
            { QStringLiteral("refused"), text(waiting, "refused") },
            { QStringLiteral("terminal"), waiting.value(QStringLiteral("terminal")).toBool() } });
    }
    if (queued != m_queued) {
        m_queued = queued;
        emit queuedChanged();
    }
    QVariantList tasks;
    const bool unfinished = std::ranges::any_of(m_chat->tasks, [](const QJsonValue& task) {
        return text(task.toObject(), "state") != QStringLiteral("done");
    });
    if (unfinished) {
        for (const auto& task : m_chat->tasks)
            tasks.append(QVariantMap { { QStringLiteral("text"), text(task.toObject(), "text") },
                { QStringLiteral("state"), text(task.toObject(), "state") } });
    }
    if (tasks != m_tasks) {
        m_tasks = tasks;
        emit tasksChanged();
    }
    QVariantList helpers;
    for (const auto& item : m_chat->items) {
        const auto tool = toolOf(item);
        if (kindOf(item) != QStringLiteral("tool") || text(tool, "kind") != QStringLiteral("agent")
            || text(tool, "state") != QStringLiteral("running"))
            continue;
        const auto detail = detailKind(tool) == QStringLiteral("agent") ? tool.value(QStringLiteral("detail")).toObject() : QJsonObject {};
        QVariantMap helper { { QStringLiteral("id"), text(item, "id") }, { QStringLiteral("kind"), text(detail, "name") },
            { QStringLiteral("task"), text(tool, "target") }, { QStringLiteral("steps"), detail.value(QStringLiteral("steps")).toInt() },
            { QStringLiteral("said"), text(detail, "said") } };
        const auto doing = detail.value(QStringLiteral("doing")).toObject();
        if (!doing.isEmpty())
            helper.insert(QStringLiteral("doing"), QVariantMap { { QStringLiteral("kind"), text(doing, "kind") },
                { QStringLiteral("target"), text(doing, "target") }, { QStringLiteral("icon"), ChatWords::icon(text(doing, "kind")) } });
        helpers.append(helper);
    }
    if (helpers != m_helpers) {
        m_helpers = helpers;
        emit helpersChanged();
    }
    const auto facts = shapedFacts();
    if (facts != m_facts) {
        m_facts = facts;
        emit factsChanged();
    }
    QVariantList commands;
    for (const auto& value : m_chat->commands) {
        const auto command = value.toObject();
        commands.append(QVariantMap { { QStringLiteral("name"), text(command, "name") },
            { QStringLiteral("description"), text(command, "description") }, { QStringLiteral("opens"), text(command, "opens") } });
    }
    if (commands != m_commands) {
        m_commands = commands;
        emit commandsChanged();
    }
}

QString TerminalChat::control(const QString& draft) const
{
    const auto words = draft.trimmed();
    if (!words.startsWith(u'/'))
        return {};
    const auto name = words.sliced(1);
    for (const auto& command : m_commands) {
        const auto values = command.toMap();
        if (values.value(QStringLiteral("name")).toString() == name)
            return values.value(QStringLiteral("opens")).toString();
    }
    return {};
}

void TerminalChat::follow(const QString& link, const QString& otherComputer)
{
    const bool home = otherComputer.isEmpty() && link.startsWith(QStringLiteral("~/"));
    const auto decision = decideLink(home ? QUrl::fromLocalFile(QDir::homePath() + link.sliced(1)).toString(QUrl::FullyEncoded) : link,
        otherComputer);
    switch (decision.action) {
    case LinkDecision::Action::Open:
        QDesktopServices::openUrl(decision.url);
        return;
    case LinkDecision::Action::Ask:
        emit linkQuestionRequested(decision.url, decision.shown);
        return;
    case LinkDecision::Action::Refuse:
        emit linkRefused(decision.shown, decision.reason);
        return;
    }
}

TerminalChats::TerminalChats(CanvasDocuments& documents, SessionCatalogModel& sessions, QObject* parent)
    : QObject(parent)
    , m_documents(documents)
    , m_sessions(sessions)
{
    connect(&documents, &CanvasDocuments::opened, this, [this](const QString& pipe) {
        if (auto* chat = byPipe(pipe))
            chat->pipeOpened();
    });
    connect(&documents, &CanvasDocuments::received, this, [this](const QString& pipe, const QString& frame) {
        if (auto* chat = byPipe(pipe))
            chat->receive(QByteArray::fromBase64(frame.toLatin1()));
    });
    connect(&documents, &CanvasDocuments::lost, this, [this](const QString& pipe, const QString& reason) {
        if (auto* chat = byPipe(pipe))
            chat->lose(reason);
    });
    connect(&documents, &CanvasDocuments::closed, this, [this](const QString& pipe) {
        if (auto* chat = byPipe(pipe))
            chat->pipeClosed();
    });
    connect(&sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &TerminalChats::forgetClosed);
}

TerminalChat* TerminalChats::chat(const QString& sessionId)
{
    auto* chat = m_chats.value(sessionId);
    if (chat == nullptr) {
        chat = new TerminalChat(m_documents, sessionId, this);
        m_chats.insert(sessionId, chat);
    }
    return chat;
}

QString TerminalChats::modelName(const QString& model, const QVariantMap& models)
{
    return ChatWords::modelName(model, models);
}

TerminalChat* TerminalChats::byPipe(const QString& pipe) const
{
    if (pipe.isEmpty())
        return nullptr;
    for (auto* chat : m_chats) {
        if (chat->pipe() == pipe)
            return chat;
    }
    return nullptr;
}

// Retain each session's chat (and unsent draft) until the session closes and no view watches it.
void TerminalChats::forgetClosed()
{
    for (auto entry = m_chats.begin(); entry != m_chats.end();) {
        if (!m_sessions.containsSession(entry.key()) && !entry.value()->watched()) {
            entry.value()->deleteLater();
            entry = m_chats.erase(entry);
        } else {
            ++entry;
        }
    }
}

}
