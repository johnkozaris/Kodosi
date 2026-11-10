#include "presentation/CanvasModel.hpp"
#include "presentation/Link.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QDateTime>
#include <QDesktopServices>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QTimer>
#include <QUuid>

#include <algorithm>
#include <array>
#include <utility>

namespace kodosi {
namespace {
constexpr qsizetype mostStoredBytes = 64 * 1024 * 1024;
constexpr qint64 mostItemBytes = 32 * 1024 * 1024;
constexpr qsizetype partBytes = 256 * 1024;
constexpr qsizetype mostWords = 600;
constexpr int mostPublished = 16;
constexpr int mostRounds = 4;
const auto browserItem = QStringLiteral("browser");
const auto pageScheme = QStringLiteral("kodosi-canvas");

quint64 number(const QVariant& value)
{
    return value.toULongLong();
}

qint64 now()
{
    return QDateTime::currentMSecsSinceEpoch();
}

std::optional<QByteArray> decoded(const QJsonValue& value)
{
    auto result = QByteArray::fromBase64Encoding(
        value.toString().toLatin1(), QByteArray::AbortOnBase64DecodingErrors);
    if (!value.isString() || !result)
        return std::nullopt;
    return std::move(*result);
}

QString oneLine(const QString& text)
{
    QString line;
    line.reserve(text.size());
    for (const auto character : text) {
        const auto category = character.category();
        const bool breaks = category == QChar::Other_Control || category == QChar::Other_Format
            || character == QChar(0x85) || character == QChar::LineSeparator || character == QChar::ParagraphSeparator;
        line.append(breaks ? QChar(u' ') : character);
    }
    return line.split(u' ', Qt::SkipEmptyParts).join(u' ');
}
}

CanvasModel::CanvasModel(CommandDispatcher& commands, SessionCatalogModel& sessions, quint64 started, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_sessions(sessions)
    , m_started(started)
{
    connect(&sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &CanvasModel::observe);
}

QString CanvasModel::storedId(const QString& sessionId, const QString& key)
{
    return sessionId + QLatin1Char('\n') + key;
}

QList<CanvasModel::Shown> CanvasModel::shown(const QString& sessionId) const
{
    const auto session = m_sessions.session(sessionId);
    if (!session || session->canvas.isEmpty())
        return {};
    QList<Shown> result;
    for (const auto& value : session->canvas.value(QStringLiteral("items")).toList()) {
        const auto item = value.toMap();
        result.append({ item.value(QStringLiteral("key")).toString(), number(item.value(QStringLiteral("changed"))) });
    }
    const auto browser = session->canvas.value(QStringLiteral("browser")).toMap();
    if (!browser.isEmpty())
        result.append({ browserItem, number(browser.value(QStringLiteral("changed"))) });
    return result;
}

std::optional<CanvasModel::Show> CanvasModel::lastShow(const QString& sessionId) const
{
    const auto session = m_sessions.session(sessionId);
    const auto open = session ? session->canvas.value(QStringLiteral("open")).toMap() : QVariantMap {};
    if (open.isEmpty())
        return std::nullopt;
    return Show { open.value(QStringLiteral("key")).toString(), number(open.value(QStringLiteral("at"))) };
}

QVariantMap CanvasModel::item(const QString& sessionId, const QString& key) const
{
    const auto session = m_sessions.session(sessionId);
    if (!session)
        return {};
    for (const auto& value : session->canvas.value(QStringLiteral("items")).toList()) {
        if (const auto item = value.toMap(); item.value(QStringLiteral("key")).toString() == key)
            return item;
    }
    return {};
}

void CanvasModel::touch()
{
    ++m_revision;
    emit changed();
}

void CanvasModel::observe()
{
    QSet<QString> ids;
    bool moved = false;
    for (const auto& value : m_sessions.sessions()) {
        const auto id = value.toMap().value(QStringLiteral("id")).toString();
        ids.insert(id);
        const auto entries = shown(id);
        if (!m_firstSeen.contains(id)) {
            quint64 newest = 0;
            for (const auto& entry : entries)
                newest = std::max(newest, entry.changed);
            m_firstSeen.insert(id, newest);
        }
        if (!m_opened.contains(id))
            m_opened.insert(id, m_started);
        const auto canvas = m_sessions.session(id).value_or(SessionCatalogModel::Session {}).canvas;
        const auto sequence = number(canvas.value(QStringLiteral("sequence")));
        if (!m_sequences.contains(id) || m_sequences.value(id) != sequence) {
            m_sequences.insert(id, sequence);
            moved = true;
        }
        if (m_opening.contains(id) && std::ranges::any_of(entries, [](const Shown& entry) { return entry.key == browserItem; })) {
            m_opening.remove(id);
            m_picks.insert(id, browserItem);
            moved = true;
        }
        const auto pick = m_picks.value(id);
        if (!pick.isEmpty() && std::ranges::none_of(entries, [&](const Shown& entry) { return entry.key == pick; })) {
            m_picks.remove(id);
            moved = true;
        }
    }
    const auto gone = [&ids](const QString& id) { return !ids.contains(id); };
    m_firstSeen.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_marks.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_sequences.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_picks.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_lastFiles.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_closed.removeIf([&](const auto& entry) { return gone(entry.key()); });
    m_opening.removeIf(gone);
    m_shelves.removeIf(gone);
    m_readings.removeIf([&](const auto& entry) { return gone(entry.value().sessionId); });
    for (const auto& id : QStringList(m_storedOrder)) {
        if (gone(id.section(QLatin1Char('\n'), 0, 0))) {
            m_stored.remove(id);
            m_storedOrder.removeOne(id);
        }
    }
    if (moved)
        touch();
}

QString CanvasModel::sectionOf(const QString& key)
{
    return key == browserItem ? browserItem : QStringLiteral("files");
}

QStringList CanvasModel::present(const QString& sessionId) const
{
    const auto entries = shown(sessionId);
    const auto browsing = [](const Shown& entry) { return entry.key == browserItem; };
    QStringList result;
    if (!std::ranges::all_of(entries, browsing))
        result.append(QStringLiteral("files"));
    if (std::ranges::any_of(entries, browsing))
        result.append(browserItem);
    return result;
}

quint64 CanvasModel::changedOf(const QString& sessionId, const QString& section) const
{
    quint64 newest = 0;
    for (const auto& entry : shown(sessionId)) {
        if (sectionOf(entry.key) == section)
            newest = std::max(newest, entry.changed);
    }
    return newest;
}

QString CanvasModel::current(const QString& sessionId) const
{
    const auto open = sections(sessionId);
    auto entries = shown(sessionId);
    entries.removeIf([&](const Shown& entry) { return !open.contains(sectionOf(entry.key)); });
    const auto pick = m_picks.value(sessionId);
    if (!pick.isEmpty() && std::ranges::any_of(entries, [&](const Shown& entry) { return entry.key == pick; }))
        return pick;
    const auto newest = std::ranges::max_element(entries, {}, &Shown::changed);
    return newest == entries.end() ? QString {} : newest->key;
}

void CanvasModel::pick(const QString& sessionId, const QString& key)
{
    const bool file = !key.isEmpty() && key != browserItem;
    if (m_picks.value(sessionId) == key && !(file && m_shelves.contains(sessionId)))
        return;
    m_picks.insert(sessionId, key);
    if (file) {
        m_lastFiles.insert(sessionId, key);
        m_shelves.remove(sessionId);
    }
    touch();
}

QStringList CanvasModel::sections(const QString& sessionId) const
{
    const auto closed = m_closed.value(sessionId);
    auto result = present(sessionId);
    result.removeIf([&](const QString& section) {
        return closed.contains(section) && changedOf(sessionId, section) <= closed.value(section);
    });
    return result;
}

bool CanvasModel::showsSomething(const QString& sessionId) const
{
    return !sections(sessionId).isEmpty();
}

QStringList CanvasModel::closedSections(const QString& sessionId) const
{
    const auto open = sections(sessionId);
    const auto there = present(sessionId);
    QStringList result;
    for (const auto& section : { QStringLiteral("files"), browserItem }) {
        if (!open.contains(section) && (section == browserItem || there.contains(section)))
            result.append(section);
    }
    return result;
}

void CanvasModel::close(const QString& sessionId, const QString& section)
{
    settle(sessionId);
    m_closed[sessionId].insert(section, changedOf(sessionId, section));
    if (const auto pick = m_picks.value(sessionId); !pick.isEmpty() && sectionOf(pick) == section)
        m_picks.remove(sessionId);
    if (section != browserItem)
        m_shelves.remove(sessionId);
    touch();
}

void CanvasModel::show(const QString& sessionId, const QString& section)
{
    settle(sessionId);
    m_closed[sessionId].remove(section);
    if (section == browserItem) {
        m_picks.insert(sessionId, browserItem);
    } else {
        const auto entries = shown(sessionId);
        const auto last = m_lastFiles.value(sessionId);
        auto files = entries;
        files.removeIf([](const Shown& entry) { return entry.key == browserItem; });
        const auto newest = std::ranges::max_element(files, {}, &Shown::changed);
        if (std::ranges::any_of(files, [&](const Shown& entry) { return entry.key == last; }))
            m_picks.insert(sessionId, last);
        else if (newest != files.end())
            m_picks.insert(sessionId, newest->key);
        else
            m_picks.remove(sessionId);
    }
    touch();
}

void CanvasModel::open(const QString& sessionId, const QString& section)
{
    if (!section.isEmpty()) {
        show(sessionId, section);
        return;
    }
    if (showsSomething(sessionId))
        return;
    const auto entries = shown(sessionId);
    if (const auto newest = std::ranges::max_element(entries, {}, &Shown::changed); newest != entries.end())
        show(sessionId, sectionOf(newest->key));
}

bool CanvasModel::showsShelf(const QString& sessionId) const
{
    return m_shelves.contains(sessionId);
}

void CanvasModel::showShelf(const QString& sessionId, bool shown)
{
    if (shown == m_shelves.contains(sessionId))
        return;
    if (shown)
        m_shelves.insert(sessionId);
    else
        m_shelves.remove(sessionId);
    touch();
}

void CanvasModel::openBrowser(const QString& sessionId)
{
    if (present(sessionId).contains(browserItem)) {
        show(sessionId, browserItem);
        return;
    }
    m_closed[sessionId].remove(browserItem);
    m_opening.insert(sessionId);
    touch();
    wake(sessionId);
}

bool CanvasModel::opensBrowser(const QString& sessionId) const
{
    return m_opening.contains(sessionId);
}

bool CanvasModel::isNew(const QString& sessionId, const QString& key) const
{
    const auto entries = shown(sessionId);
    const auto entry = std::ranges::find(entries, key, &Shown::key);
    if (entry == entries.end())
        return false;
    const auto marks = m_marks.value(sessionId);
    return entry->changed > (marks.contains(key) ? marks.value(key) : m_firstSeen.value(sessionId));
}

void CanvasModel::saw(const QString& sessionId, const QString& key)
{
    if (!isNew(sessionId, key))
        return;
    const auto entries = shown(sessionId);
    m_marks[sessionId].insert(key, std::ranges::find(entries, key, &Shown::key)->changed);
    touch();
}

bool CanvasModel::showsNew(const QString& sessionId) const
{
    const auto show = lastShow(sessionId);
    if (!show || show->at <= m_opened.value(sessionId, m_started))
        return false;
    const auto entries = shown(sessionId);
    return std::ranges::find(entries, show->key, &Shown::key) != entries.end();
}

bool CanvasModel::openShown(const QString& sessionId)
{
    if (!showsNew(sessionId))
        return false;
    const auto show = *lastShow(sessionId);
    m_opened.insert(sessionId, show.at);
    m_closed[sessionId].remove(sectionOf(show.key));
    pick(sessionId, show.key);
    return true;
}

void CanvasModel::settle(const QString& sessionId)
{
    if (const auto show = lastShow(sessionId))
        m_opened.insert(sessionId, std::max(m_opened.value(sessionId, m_started), show->at));
}

void CanvasModel::remove(const QString& sessionId, const QString& key)
{
    if (const auto error = ask(Asked::Remove, sessionId,
            { { QStringLiteral("type"), QStringLiteral("remove") },
                { QStringLiteral("keys"), QJsonArray { key } } },
            key);
        !error.isEmpty())
        emit problem(error);
}

QString CanvasModel::question(const QString& sessionId, const QString& key, const QString& words) const
{
    if (key != browserItem)
        return about(key, words);
    const auto session = m_sessions.session(sessionId);
    QString place = tr("the shared browser");
    for (const auto& value : session ? session->canvas.value(QStringLiteral("browser")).toMap().value(QStringLiteral("tabs")).toList()
                                     : QVariantList {}) {
        if (const auto tab = value.toMap(); tab.value(QStringLiteral("active")).toBool())
            place = tab.value(QStringLiteral("url")).toString();
    }
    return about(place, words);
}

QString CanvasModel::about(const QString& place, const QString& words)
{
    auto line = oneLine(words);
    if (line.size() > mostWords)
        line.truncate(line.at(mostWords - 1).isHighSurrogate() ? mostWords - 1 : mostWords);
    return line.isEmpty() ? tr("About %1: ").arg(place) : tr("About %1: \"%2\" ").arg(place, line);
}

// A link on a page or in a file of the canvas opens by the rules of terminal links.
void CanvasModel::follow(const QString& sessionId, const QUrl& link)
{
    const auto session = m_sessions.session(sessionId);
    const auto elsewhere = session && session->kind != QStringLiteral("local")
        ? SessionCatalogModel::computerOf(*session)
        : QString();
    const auto decision = decideLink(link.toString(QUrl::FullyEncoded), elsewhere);
    switch (decision.action) {
    case LinkDecision::Action::Open:
        QDesktopServices::openUrl(decision.url);
        return;
    case LinkDecision::Action::Ask:
        emit linkQuestionRequested(sessionId, decision.url, decision.shown);
        return;
    case LinkDecision::Action::Refuse:
        emit linkRefused(sessionId, decision.shown, decision.reason);
        return;
    }
}

bool CanvasModel::load(const QString& sessionId, const QString& key)
{
    const auto item = this->item(sessionId, key);
    if (item.isEmpty() || item.value(QStringLiteral("missing")).toBool())
        return false;
    const auto id = storedId(sessionId, key);
    if (const auto found = m_stored.constFind(id);
        found != m_stored.cend() && found->revision >= number(item.value(QStringLiteral("revision")))) {
        m_storedOrder.removeOne(id);
        m_storedOrder.append(id);
        return true;
    }
    read(sessionId, key, false);
    return false;
}

void CanvasModel::read(const QString& sessionId, const QString& key, bool copy)
{
    const auto id = storedId(sessionId, key);
    if (const auto found = m_readings.find(id); found != m_readings.end()) {
        found->copy = found->copy || copy;
        return;
    }
    m_readings.insert(id,
        { sessionId, key, number(item(sessionId, key).value(QStringLiteral("revision"))), {}, std::nullopt, 0, copy });
    askPart(id);
}

void CanvasModel::askPart(const QString& id)
{
    const auto& reading = m_readings[id];
    const auto error = ask(Asked::Read, reading.sessionId,
        { { QStringLiteral("type"), QStringLiteral("read") }, { QStringLiteral("key"), reading.key },
            { QStringLiteral("revision"), static_cast<qint64>(reading.revision) },
            { QStringLiteral("offset"), reading.data.size() } },
        reading.key);
    if (!error.isEmpty())
        endReading(id, error);
}

void CanvasModel::receivePart(const Request& request, const QJsonObject& answer)
{
    const auto id = storedId(request.sessionId, request.key);
    const auto found = m_readings.find(id);
    if (found == m_readings.end())
        return;
    auto& reading = *found;
    const auto kind = answer.value(QStringLiteral("answer")).toString();
    if (kind == QStringLiteral("stale")) {
        const auto newer = answer.value(QStringLiteral("revision")).toInteger(-1);
        if (newer < 0 || ++reading.rounds >= mostRounds) {
            endReading(id, tr("This item kept changing while Kodosi read it."));
            return;
        }
        reading.revision = static_cast<quint64>(newer);
        reading.data.clear();
        reading.total.reset();
        askPart(id);
        return;
    }
    const auto offset = answer.value(QStringLiteral("offset")).toInteger(-1);
    const auto total = answer.value(QStringLiteral("total")).toInteger(-1);
    const auto part = decoded(answer.value(QStringLiteral("data")));
    if (kind != QStringLiteral("part") || offset != reading.data.size() || total < 0 || total > mostItemBytes || !part
        || reading.data.size() + part->size() > total || (part->isEmpty() && total != reading.data.size())) {
        endReading(id, tr("Kodosi got a canvas answer it cannot read."));
        return;
    }
    reading.data += *part;
    reading.total = static_cast<quint64>(total);
    if (reading.data.size() < total) {
        askPart(id);
        return;
    }
    const auto done = m_readings.take(id);
    store(id, done.revision, done.data);
    emit loaded(done.sessionId, done.key);
    if (done.copy)
        emit copyReady(item(done.sessionId, done.key).value(QStringLiteral("name")).toString(), done.data);
}

void CanvasModel::endReading(const QString& id, const QString& failure)
{
    const auto reading = m_readings.take(id);
    emit failed(reading.sessionId, reading.key, failure);
    if (reading.copy)
        emit problem(failure);
}

void CanvasModel::store(const QString& id, quint64 revision, QByteArray data)
{
    m_storedOrder.removeOne(id);
    m_stored.insert(id, { revision, std::move(data) });
    m_storedOrder.append(id);
    qsizetype bytes = 0;
    for (const auto& stored : std::as_const(m_stored))
        bytes += stored.data.size();
    while (bytes > mostStoredBytes && m_storedOrder.size() > 1)
        bytes -= m_stored.take(m_storedOrder.takeFirst()).data.size();
}

std::optional<QByteArray> CanvasModel::stored(const QString& sessionId, const QString& key) const
{
    const auto found = m_stored.constFind(storedId(sessionId, key));
    return found == m_stored.cend() ? std::nullopt : std::optional(found->data);
}

int CanvasModel::publish(const QString& sessionId, const QString& key)
{
    const auto source = storedId(sessionId, key);
    const auto content = m_stored.constFind(source);
    if (content == m_stored.cend())
        return 0;
    const auto shown = std::ranges::find_if(m_published, [&](const Published& page) {
        return page.source == source && page.revision == content->revision;
    });
    auto page = shown == m_published.end() ? Published { ++m_publishedCount, source, content->revision, {}, content->data }
                                           : *shown;
    if (shown != m_published.end())
        m_published.erase(shown);
    page.type = item(sessionId, key).value(QStringLiteral("kind")).toString() == QStringLiteral("pdf")
        ? QByteArrayLiteral("application/pdf")
        : QByteArrayLiteral("text/html;charset=utf-8");
    m_published.append(std::move(page));
    while (m_published.size() > mostPublished)
        m_published.removeFirst();
    return m_published.last().number;
}

QString CanvasModel::pageAddress(const QString& sessionId, const QString& key)
{
    const auto number = publish(sessionId, key);
    return number == 0 ? QString {} : QStringLiteral("%1://page/%2").arg(pageScheme).arg(number);
}

QString CanvasModel::pictureAddress(const QString& sessionId, const QString& key)
{
    const auto number = publish(sessionId, key);
    return number == 0 ? QString {} : QStringLiteral("image://canvas/item/%1").arg(number);
}

std::optional<QByteArray> CanvasModel::published(int number) const
{
    const auto found = std::ranges::find(m_published, number, &Published::number);
    return found == m_published.end() ? std::nullopt : std::optional(found->data);
}

std::optional<std::pair<QByteArray, QByteArray>> CanvasModel::published(const QUrl& address) const
{
    const auto path = address.path();
    if (address.scheme() != pageScheme || address.host() != QStringLiteral("page") || !path.startsWith(QLatin1Char('/')))
        return std::nullopt;
    bool valid = false;
    const auto number = path.mid(1).toInt(&valid);
    const auto found = std::ranges::find(m_published, number, &Published::number);
    if (!valid || found == m_published.end())
        return std::nullopt;
    return std::pair(found->type, found->data);
}

void CanvasModel::saveCopy(const QString& sessionId, const QString& key)
{
    const auto item = this->item(sessionId, key);
    if (item.isEmpty())
        return;
    const auto found = m_stored.constFind(storedId(sessionId, key));
    if (found != m_stored.cend() && found->revision >= number(item.value(QStringLiteral("revision"))))
        emit copyReady(item.value(QStringLiteral("name")).toString(), found->data);
    else
        read(sessionId, key, true);
}

void CanvasModel::attach(const QString& sessionId, const QUrl& file)
{
    upload(sessionId, file, false);
}

// The files of a page go into the folder of the terminal first, because the browser runs on the
// computer of the terminal.
void CanvasModel::giveFiles(const QString& sessionId, const QList<QUrl>& files)
{
    auto& giving = m_giving[sessionId];
    giving.waiting = 1;
    giving.paths.clear();
    for (const auto& file : files)
        giving.waiting += upload(sessionId, file, true) ? 1 : 0;
    gave(sessionId, {});
}

void CanvasModel::gave(const QString& sessionId, const QString& path)
{
    const auto found = m_giving.find(sessionId);
    if (found == m_giving.end())
        return;
    if (!path.isEmpty())
        found->paths.append(path);
    if (--found->waiting == 0)
        emit filesGiven(sessionId, m_giving.take(sessionId).paths);
}

bool CanvasModel::upload(const QString& sessionId, const QUrl& file, const bool forPage)
{
    QFile source(file.toLocalFile());
    if (!file.isLocalFile() || !source.open(QIODevice::ReadOnly)) {
        emit problem(tr("Kodosi could not read this file."));
        return false;
    }
    if (source.size() > mostItemBytes) {
        emit problem(tr("The canvas takes files up to 32 MB."));
        return false;
    }
    const auto upload = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    m_uploads.insert(upload, { sessionId, QFileInfo(source).fileName(), source.readAll(), 0, forPage });
    return sendPart(upload);
}

bool CanvasModel::sendPart(const QString& upload)
{
    const auto found = m_uploads.find(upload);
    if (found == m_uploads.end())
        return false;
    const auto part = found->data.mid(found->offset, partBytes);
    const QJsonObject request { { QStringLiteral("type"), QStringLiteral("attach") },
        { QStringLiteral("upload"), upload }, { QStringLiteral("name"), found->name },
        { QStringLiteral("offset"), static_cast<qint64>(found->offset) },
        { QStringLiteral("data"), QString::fromLatin1(part.toBase64()) },
        { QStringLiteral("last"), found->offset + part.size() >= found->data.size() },
        { QStringLiteral("page"), found->forPage } };
    found->offset += part.size();
    if (const auto error = ask(Asked::Attach, found->sessionId, request, upload); !error.isEmpty()) {
        m_uploads.erase(found);
        emit problem(error);
        return false;
    }
    return true;
}

void CanvasModel::openBrowserSettings()
{
    m_browserDone.clear();
    m_browserProblem.clear();
    emit browserChanged();
    askBrowserStatus();
}

void CanvasModel::refreshBrowser()
{
    askBrowserStatus();
}

void CanvasModel::askBrowserStatus()
{
    m_browserStatusRequest = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    const QJsonObject fields { { QStringLiteral("type"), QStringLiteral("browser.status") },
        { QStringLiteral("requestId"), m_browserStatusRequest } };
    if (const auto sent = m_commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)); !sent) {
        m_browserStatusRequest.clear();
        m_browserProblem = sent.error().message;
        emit browserChanged();
    }
}

void CanvasModel::browserStep(const QString& key, const QString& type, QJsonObject fields)
{
    if (!m_browserWork.isEmpty())
        return;
    m_browserStepRequest = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    m_browserWork = key;
    m_browserProblem.clear();
    fields.insert(QStringLiteral("type"), type);
    fields.insert(QStringLiteral("requestId"), m_browserStepRequest);
    if (const auto sent = m_commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)); !sent) {
        m_browserStepRequest.clear();
        m_browserWork.clear();
        m_browserProblem = sent.error().message;
    }
    emit browserChanged();
}

void CanvasModel::getBrowser()
{
    if (!m_browserWork.isEmpty())
        return;
    m_browserChecked = m_browserEngine;
    m_browserDone.remove(QStringLiteral("engine"));
    browserStep(QStringLiteral("engine"), QStringLiteral("browser.get"));
}

void CanvasModel::copySignIns(const QString& source)
{
    browserStep(source, QStringLiteral("browser.copy"), { { QStringLiteral("name"), source } });
}

void CanvasModel::forgetSignIns()
{
    browserStep(QStringLiteral("forget"), QStringLiteral("browser.forget"));
}

void CanvasModel::removeBrowsingData()
{
    browserStep(QStringLiteral("clear"), QStringLiteral("browser.clear"));
}

void CanvasModel::applyBrowser(const QJsonObject& event)
{
    const auto request = event.value(QStringLiteral("requestId")).toString();
    if (request.isEmpty() || (request != m_browserStatusRequest && request != m_browserStepRequest))
        return;
    const auto failed = event.value(QStringLiteral("type")).toString() == QStringLiteral("browser.error");
    if (failed)
        m_browserProblem = event.value(QStringLiteral("message")).toString();
    if (request == m_browserStatusRequest) {
        m_browserStatusRequest.clear();
        if (!failed) {
            m_browserSources.clear();
            const auto sources = event.value(QStringLiteral("sources")).toArray();
            for (const auto& source : sources)
                m_browserSources.append(source.toObject().value(QStringLiteral("name")).toString());
            m_browserSites = event.value(QStringLiteral("sites")).toInt();
            m_browserEngine = event.value(QStringLiteral("engine")).toString();
            const auto getting = event.value(QStringLiteral("getting"));
            m_browserGetting = getting.isDouble() ? getting.toInt() : -1;
            m_browserKnown = true;
            if (m_browserChecked && m_browserWork.isEmpty()) {
                if (!m_browserChecked->isEmpty() && *m_browserChecked == m_browserEngine)
                    m_browserDone.insert(QStringLiteral("engine"), tr("Up to date"));
                m_browserChecked.reset();
            }
        }
        emit browserChanged();
        return;
    }
    m_browserStepRequest.clear();
    const auto key = std::exchange(m_browserWork, {});
    const auto operation = event.value(QStringLiteral("operation")).toString();
    if (failed)
        m_browserChecked.reset();
    if (!failed && operation == QStringLiteral("browser.copy")) {
        m_browserDone.insert(key, tr("Copied for %n sites", nullptr, event.value(QStringLiteral("sites")).toInt()));
    } else if (!failed && operation != QStringLiteral("browser.get")) {
        m_browserDone.clear();
        if (operation == QStringLiteral("browser.clear"))
            m_browserDone.insert(key, tr("Removed"));
    }
    emit browserChanged();
    askBrowserStatus();
}

void CanvasModel::act(const QString& sessionId, const QVariantMap& action)
{
    if (const auto error = ask(Asked::Act, sessionId,
            { { QStringLiteral("type"), QStringLiteral("act") },
                { QStringLiteral("action"), QJsonObject::fromVariantMap(action) } });
        !error.isEmpty())
        emit problem(error);
}

void CanvasModel::wake(const QString& sessionId)
{
    act(sessionId, { { QStringLiteral("do"), QStringLiteral("wake") } });
}

std::optional<QPointF> CanvasModel::browserPoint(QPointF point, QSizeF view, QSizeF browser)
{
    if (browser.width() <= 0 || browser.height() <= 0 || view.width() <= 0 || view.height() <= 0)
        return std::nullopt;
    const auto fit = std::min(view.width() / browser.width(), view.height() / browser.height());
    const QPointF origin((view.width() - browser.width() * fit) / 2, (view.height() - browser.height() * fit) / 2);
    const auto x = (point.x() - origin.x()) / fit;
    const auto y = (point.y() - origin.y()) / fit;
    if (x < 0 || y < 0 || x >= browser.width() || y >= browser.height())
        return std::nullopt;
    return QPointF(x, y);
}

std::optional<QPointF> CanvasModel::pointInView(QPointF point, QSizeF view, QSizeF browser)
{
    if (browser.width() <= 0 || browser.height() <= 0 || view.width() <= 0 || view.height() <= 0)
        return std::nullopt;
    const auto fit = std::min(view.width() / browser.width(), view.height() / browser.height());
    const QPointF origin((view.width() - browser.width() * fit) / 2, (view.height() - browser.height() * fit) / 2);
    return QPointF(origin.x() + point.x() * fit, origin.y() + point.y() * fit);
}

std::optional<CanvasModel::BrowserKey> CanvasModel::browserKey(int key)
{
    if (key >= Qt::Key_A && key <= Qt::Key_Z)
        return BrowserKey { QStringLiteral("Key") + QChar(u'A' + (key - Qt::Key_A)), {}, 65 + key - Qt::Key_A };
    if (key >= Qt::Key_0 && key <= Qt::Key_9)
        return BrowserKey { QStringLiteral("Digit") + QChar(u'0' + (key - Qt::Key_0)), {}, 48 + key - Qt::Key_0 };
    if (key >= Qt::Key_F1 && key <= Qt::Key_F12) {
        const auto name = QStringLiteral("F%1").arg(key - Qt::Key_F1 + 1);
        return BrowserKey { name, name, 112 + key - Qt::Key_F1 };
    }
    struct Entry {
        int key;
        const char16_t* code;
        const char16_t* named;
        int windows;
    };
    static constexpr std::array entries {
        Entry { Qt::Key_Exclam, u"Digit1", nullptr, 49 }, Entry { Qt::Key_At, u"Digit2", nullptr, 50 },
        Entry { Qt::Key_NumberSign, u"Digit3", nullptr, 51 }, Entry { Qt::Key_Dollar, u"Digit4", nullptr, 52 },
        Entry { Qt::Key_Percent, u"Digit5", nullptr, 53 }, Entry { Qt::Key_AsciiCircum, u"Digit6", nullptr, 54 },
        Entry { Qt::Key_Ampersand, u"Digit7", nullptr, 55 }, Entry { Qt::Key_Asterisk, u"Digit8", nullptr, 56 },
        Entry { Qt::Key_ParenLeft, u"Digit9", nullptr, 57 }, Entry { Qt::Key_ParenRight, u"Digit0", nullptr, 48 },
        Entry { Qt::Key_Minus, u"Minus", nullptr, 189 }, Entry { Qt::Key_Underscore, u"Minus", nullptr, 189 },
        Entry { Qt::Key_Equal, u"Equal", nullptr, 187 }, Entry { Qt::Key_Plus, u"Equal", nullptr, 187 },
        Entry { Qt::Key_BracketLeft, u"BracketLeft", nullptr, 219 }, Entry { Qt::Key_BraceLeft, u"BracketLeft", nullptr, 219 },
        Entry { Qt::Key_BracketRight, u"BracketRight", nullptr, 221 },
        Entry { Qt::Key_BraceRight, u"BracketRight", nullptr, 221 }, Entry { Qt::Key_Backslash, u"Backslash", nullptr, 220 },
        Entry { Qt::Key_Bar, u"Backslash", nullptr, 220 }, Entry { Qt::Key_Semicolon, u"Semicolon", nullptr, 186 },
        Entry { Qt::Key_Colon, u"Semicolon", nullptr, 186 }, Entry { Qt::Key_Apostrophe, u"Quote", nullptr, 222 },
        Entry { Qt::Key_QuoteDbl, u"Quote", nullptr, 222 }, Entry { Qt::Key_QuoteLeft, u"Backquote", nullptr, 192 },
        Entry { Qt::Key_AsciiTilde, u"Backquote", nullptr, 192 }, Entry { Qt::Key_Comma, u"Comma", nullptr, 188 },
        Entry { Qt::Key_Less, u"Comma", nullptr, 188 }, Entry { Qt::Key_Period, u"Period", nullptr, 190 },
        Entry { Qt::Key_Greater, u"Period", nullptr, 190 }, Entry { Qt::Key_Slash, u"Slash", nullptr, 191 },
        Entry { Qt::Key_Question, u"Slash", nullptr, 191 }, Entry { Qt::Key_Return, u"Enter", u"Enter", 13 },
        Entry { Qt::Key_Enter, u"NumpadEnter", u"Enter", 13 }, Entry { Qt::Key_Escape, u"Escape", u"Escape", 27 },
        Entry { Qt::Key_Backspace, u"Backspace", u"Backspace", 8 }, Entry { Qt::Key_Tab, u"Tab", u"Tab", 9 },
        Entry { Qt::Key_Backtab, u"Tab", u"Tab", 9 }, Entry { Qt::Key_Space, u"Space", u" ", 32 },
        Entry { Qt::Key_Insert, u"Insert", u"Insert", 45 }, Entry { Qt::Key_Delete, u"Delete", u"Delete", 46 },
        Entry { Qt::Key_Home, u"Home", u"Home", 36 }, Entry { Qt::Key_End, u"End", u"End", 35 },
        Entry { Qt::Key_PageUp, u"PageUp", u"PageUp", 33 }, Entry { Qt::Key_PageDown, u"PageDown", u"PageDown", 34 },
        Entry { Qt::Key_Left, u"ArrowLeft", u"ArrowLeft", 37 }, Entry { Qt::Key_Up, u"ArrowUp", u"ArrowUp", 38 },
        Entry { Qt::Key_Right, u"ArrowRight", u"ArrowRight", 39 }, Entry { Qt::Key_Down, u"ArrowDown", u"ArrowDown", 40 },
    };
    const auto found = std::ranges::find(entries, key, &Entry::key);
    if (found == entries.end())
        return std::nullopt;
    return BrowserKey { QString::fromUtf16(found->code), found->named ? QString::fromUtf16(found->named) : QString {},
        found->windows };
}

QString CanvasModel::ask(Asked asked, const QString& sessionId, QJsonObject request, const QString& key)
{
    const auto id = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    const QJsonObject command { { QStringLiteral("type"), QStringLiteral("canvas.request") },
        { QStringLiteral("requestId"), id }, { QStringLiteral("sessionId"), sessionId },
        { QStringLiteral("request"), request } };
    if (const auto sent = m_commands.send(QJsonDocument(command).toJson(QJsonDocument::Compact)); !sent)
        return sent.error().message;
    m_requests.insert(id, { asked, sessionId, key, now() });
    return {};
}

void CanvasModel::apply(const QJsonObject& event)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    const auto found = m_requests.constFind(event.value(QStringLiteral("requestId")).toString());
    if ((type != QStringLiteral("canvas.result") && type != QStringLiteral("canvas.error")) || found == m_requests.cend())
        return;
    const auto request = *found;
    m_requests.erase(found);
    const bool failed = type == QStringLiteral("canvas.error");
    const auto message = event.value(QStringLiteral("message")).toString();
    const auto answer = event.value(QStringLiteral("answer")).toObject();
    switch (request.asked) {
    case Asked::Read:
        if (failed)
            endReading(storedId(request.sessionId, request.key), message);
        else
            receivePart(request, answer);
        return;
    case Asked::Act:
        if (failed && m_opening.remove(request.sessionId)) {
            emit problem(message);
            touch();
        } else if (failed) {
            emit problem(message);
        }
        return;
    case Asked::Remove:
        if (failed)
            emit problem(message);
        return;
    case Asked::Attach:
        if (!failed && !answer.contains(QStringLiteral("key"))) {
            if (const auto forPage = m_uploads.value(request.key).forPage; !sendPart(request.key) && forPage)
                gave(request.sessionId, {});
            return;
        }
        if (const auto upload = m_uploads.take(request.key); failed) {
            emit problem(message);
            if (upload.forPage)
                gave(request.sessionId, {});
        } else if (upload.forPage) {
            gave(request.sessionId, answer.value(QStringLiteral("key")).toString());
        } else {
            show(request.sessionId, QStringLiteral("files"));
            pick(request.sessionId, answer.value(QStringLiteral("key")).toString());
        }
        return;
    }
}

void CanvasModel::reset()
{
    m_marks.clear();
    m_firstSeen.clear();
    m_opened.clear();
    m_sequences.clear();
    m_picks.clear();
    m_lastFiles.clear();
    m_closed.clear();
    m_opening.clear();
    m_shelves.clear();
    m_stored.clear();
    m_storedOrder.clear();
    m_readings.clear();
    m_requests.clear();
    m_uploads.clear();
    m_giving.clear();
    m_published.clear();
    m_browserChecked.reset();
    m_browserStatusRequest.clear();
    m_browserStepRequest.clear();
    m_browserWork.clear();
    emit browserChanged();
    touch();
}
}
