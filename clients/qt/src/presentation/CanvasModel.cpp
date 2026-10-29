#include "presentation/CanvasModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QClipboard>
#include <QDateTime>
#include <QDesktopServices>
#include <QGuiApplication>
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
constexpr qsizetype mostEvents = 64;
constexpr qsizetype mostTyped = 16 * 1024;
constexpr qsizetype mostWords = 600;
constexpr int mostPublished = 16;
constexpr int mostRounds = 4;
constexpr qint64 pictureRest = 1000;
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

CanvasModel::CanvasModel(CommandDispatcher& commands, SessionCatalogModel& sessions, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_sessions(sessions)
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
        if (canvas.value(QStringLiteral("browser")).toMap().isEmpty())
            m_feeds.remove(id);
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
    m_feeds.removeIf([&](const auto& entry) { return gone(entry.key()); });
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
    m_closed[sessionId].insert(section, changedOf(sessionId, section));
    if (const auto pick = m_picks.value(sessionId); !pick.isEmpty() && sectionOf(pick) == section)
        m_picks.remove(sessionId);
    if (section != browserItem)
        m_shelves.remove(sessionId);
    touch();
}

void CanvasModel::show(const QString& sessionId, const QString& section)
{
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

bool CanvasModel::hasNews(const QString& sessionId) const
{
    const auto entries = shown(sessionId);
    return std::ranges::any_of(entries, [&](const Shown& entry) { return isNew(sessionId, entry.key); });
}

void CanvasModel::saw(const QString& sessionId, const QString& key)
{
    if (!isNew(sessionId, key))
        return;
    const auto entries = shown(sessionId);
    m_marks[sessionId].insert(key, std::ranges::find(entries, key, &Shown::key)->changed);
    touch();
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
    return line.isEmpty() ? QStringLiteral("About %1: ").arg(place) : QStringLiteral("About %1: \"%2\" ").arg(place, line);
}

bool CanvasModel::followable(const QUrl& link, bool otherComputer)
{
    const auto scheme = link.scheme().toLower();
    if (scheme == QStringLiteral("mailto"))
        return link.isValid();
    if (!link.isValid() || (scheme != QStringLiteral("http") && scheme != QStringLiteral("https")) || link.host().isEmpty())
        return false;
    const auto host = link.host().toLower();
    return !otherComputer
        || (host != QStringLiteral("localhost") && host != QStringLiteral("127.0.0.1") && host != QStringLiteral("::1")
            && !host.endsWith(QStringLiteral(".localhost")));
}

bool CanvasModel::follow(const QUrl& link, bool otherComputer)
{
    return followable(link, otherComputer) && QDesktopServices::openUrl(link);
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

void CanvasModel::watch(const QString& sessionId)
{
    auto& feed = m_feeds[sessionId];
    ++feed.watchers;
    if (!feed.asking)
        askPicture(sessionId);
}

void CanvasModel::unwatch(const QString& sessionId)
{
    if (const auto found = m_feeds.find(sessionId); found != m_feeds.end())
        found->watchers = std::max(0, found->watchers - 1);
}

void CanvasModel::askPicture(const QString& sessionId)
{
    auto& feed = m_feeds[sessionId];
    feed.asking = true;
    const auto sent = now();
    if (const auto error = ask(Asked::Picture, sessionId,
            { { QStringLiteral("type"), QStringLiteral("picture") },
                { QStringLiteral("after"), static_cast<qint64>(feed.sequence) } });
        !error.isEmpty()) {
        feed.asking = false;
        failBrowser(sessionId, error);
        retryPicture(sessionId, sent);
    }
}

void CanvasModel::receivePicture(const QString& sessionId, const QJsonObject& answer, qint64 sent)
{
    const auto feed = m_feeds.find(sessionId);
    if (feed == m_feeds.end())
        return;
    feed->asking = false;
    const auto sequence = answer.value(QStringLiteral("sequence")).toInteger(-1);
    if (answer.value(QStringLiteral("answer")).toString() != QStringLiteral("picture") || sequence < 0) {
        failBrowser(sessionId, tr("Kodosi got a canvas answer it cannot read."));
        retryPicture(sessionId, sent);
        return;
    }
    const auto frame = answer.value(QStringLiteral("frame")).toObject();
    if (frame.isEmpty()) {
        retryPicture(sessionId, sent);
        return;
    }
    const auto jpeg = decoded(frame.value(QStringLiteral("data")));
    QImage image;
    const auto width = frame.value(QStringLiteral("width")).toInteger();
    const auto height = frame.value(QStringLiteral("height")).toInteger();
    if (!jpeg || !image.loadFromData(*jpeg, "JPEG") || width <= 0 || height <= 0) {
        failBrowser(sessionId, tr("Kodosi got a canvas answer it cannot read."));
        retryPicture(sessionId, sent);
        return;
    }
    feed->sequence = static_cast<quint64>(sequence);
    feed->image = std::move(image);
    feed->browser = QSizeF(static_cast<qreal>(width), static_cast<qreal>(height));
    feed->failure.clear();
    emit pictureChanged(sessionId);
    if (feed->watchers > 0)
        askPicture(sessionId);
}

void CanvasModel::retryPicture(const QString& sessionId, qint64 sent)
{
    QTimer::singleShot(std::max<qint64>(0, pictureRest - (now() - sent)), this, [this, sessionId] {
        const auto feed = m_feeds.constFind(sessionId);
        if (feed != m_feeds.cend() && feed->watchers > 0 && !feed->asking)
            askPicture(sessionId);
    });
}

void CanvasModel::failBrowser(const QString& sessionId, const QString& message)
{
    m_feeds[sessionId].failure = message;
    emit pictureChanged(sessionId);
}

QVariantMap CanvasModel::picture(const QString& sessionId) const
{
    const auto feed = m_feeds.value(sessionId);
    return { { QStringLiteral("sequence"), feed.sequence }, { QStringLiteral("shows"), !feed.image.isNull() },
        { QStringLiteral("failure"), feed.failure } };
}

QImage CanvasModel::browserImage(const QString& sessionId) const
{
    return m_feeds.value(sessionId).image;
}

void CanvasModel::setBrowserView(const QString& sessionId, qreal width, qreal height)
{
    m_feeds[sessionId].view = QSizeF(width, height);
}

void CanvasModel::act(const QString& sessionId, const QVariantMap& action)
{
    if (const auto error = ask(Asked::Act, sessionId,
            { { QStringLiteral("type"), QStringLiteral("act") },
                { QStringLiteral("action"), QJsonObject::fromVariantMap(action) } });
        !error.isEmpty())
        failBrowser(sessionId, error);
}

void CanvasModel::wake(const QString& sessionId)
{
    const auto view = m_feeds.value(sessionId).view;
    act(sessionId,
        { { QStringLiteral("do"), QStringLiteral("wake") }, { QStringLiteral("width"), qRound(view.width()) },
            { QStringLiteral("height"), qRound(view.height()) } });
}

std::optional<QPoint> CanvasModel::browserPoint(QPointF point, QSizeF view, QSizeF picture, QSizeF browser)
{
    if (picture.width() <= 0 || picture.height() <= 0 || browser.height() <= 0 || view.width() <= 0
        || view.height() <= 0)
        return std::nullopt;
    const auto fit = std::min(view.width() / picture.width(), view.height() / picture.height());
    const QPointF origin((view.width() - picture.width() * fit) / 2, (view.height() - picture.height() * fit) / 2);
    const auto perPixel = picture.height() / browser.height();
    const auto x = (point.x() - origin.x()) / fit / perPixel;
    const auto y = (point.y() - origin.y()) / fit / perPixel;
    if (x < 0 || y < 0 || x >= browser.width() || y >= browser.height())
        return std::nullopt;
    return QPoint(static_cast<int>(x), static_cast<int>(y));
}

std::optional<QPointF> CanvasModel::pointInView(QPoint point, QSizeF view, QSizeF picture, QSizeF browser)
{
    if (picture.width() <= 0 || picture.height() <= 0 || browser.height() <= 0 || view.width() <= 0
        || view.height() <= 0)
        return std::nullopt;
    const auto fit = std::min(view.width() / picture.width(), view.height() / picture.height());
    const QPointF origin((view.width() - picture.width() * fit) / 2, (view.height() - picture.height() * fit) / 2);
    const auto perPixel = picture.height() / browser.height();
    return QPointF(origin.x() + point.x() * perPixel * fit, origin.y() + point.y() * perPixel * fit);
}

QVariant CanvasModel::viewPoint(const QString& sessionId, int x, int y) const
{
    const auto feed = m_feeds.constFind(sessionId);
    if (feed == m_feeds.cend())
        return {};
    const auto at = pointInView(QPoint(x, y), feed->view, QSizeF(feed->image.size()), feed->browser);
    return at ? QVariant(*at) : QVariant {};
}

std::optional<QPoint> CanvasModel::point(const QString& sessionId, qreal x, qreal y) const
{
    const auto feed = m_feeds.constFind(sessionId);
    if (feed == m_feeds.cend())
        return std::nullopt;
    return browserPoint(QPointF(x, y), feed->view, QSizeF(feed->image.size()), feed->browser);
}

int CanvasModel::browserModifiers(int modifiers)
{
    return (modifiers & Qt::AltModifier ? 1 : 0) | (modifiers & Qt::ControlModifier ? 2 : 0)
        | (modifiers & Qt::MetaModifier ? 4 : 0) | (modifiers & Qt::ShiftModifier ? 8 : 0);
}

void CanvasModel::pointer(const QString& sessionId, const QString& type, qreal x, qreal y, int button, int clicks,
    int modifiers)
{
    const auto at = point(sessionId, x, y);
    if (!at)
        return;
    const auto name = button == Qt::LeftButton ? QStringLiteral("left")
        : button == Qt::RightButton            ? QStringLiteral("right")
        : button == Qt::MiddleButton           ? QStringLiteral("middle")
                                               : QStringLiteral("none");
    send(sessionId,
        { { QStringLiteral("type"), QStringLiteral("mouse") }, { QStringLiteral("eventType"), type },
            { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() }, { QStringLiteral("button"), name },
            { QStringLiteral("clickCount"), type == QStringLiteral("mouseMoved") ? 0 : std::clamp(clicks, 0, 3) },
            { QStringLiteral("deltaX"), 0 }, { QStringLiteral("deltaY"), 0 },
            { QStringLiteral("modifiers"), browserModifiers(modifiers) } });
}

void CanvasModel::wheel(const QString& sessionId, qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers)
{
    const auto at = point(sessionId, x, y);
    if (!at)
        return;
    const auto delta = pixels.isNull() ? angle * 100 / 120 : pixels;
    send(sessionId,
        { { QStringLiteral("type"), QStringLiteral("mouse") }, { QStringLiteral("eventType"), QStringLiteral("mouseWheel") },
            { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() },
            { QStringLiteral("button"), QStringLiteral("none") }, { QStringLiteral("clickCount"), 0 },
            { QStringLiteral("deltaX"), -delta.x() }, { QStringLiteral("deltaY"), -delta.y() },
            { QStringLiteral("modifiers"), browserModifiers(modifiers) } });
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

bool CanvasModel::key(const QString& sessionId, int key, const QString& text, int modifiers, bool down)
{
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    const auto printable = !text.isEmpty() && std::ranges::all_of(text, [](QChar character) { return character.isPrint(); });
    const auto found = browserKey(key);
    if (!found) {
        if (down && printable && !command)
            type(sessionId, text);
        return down && printable && !command;
    }
    QString typed = printable ? text : QString {};
    if (key >= Qt::Key_A && key <= Qt::Key_Z && !printable)
        typed = modifiers & Qt::ShiftModifier ? QChar(u'A' + (key - Qt::Key_A)) : QChar(u'a' + (key - Qt::Key_A));
    else if (key >= Qt::Key_0 && key <= Qt::Key_9 && !printable)
        typed = QChar(u'0' + (key - Qt::Key_0));
    press(sessionId, *found, typed, down, modifiers);
    return true;
}

void CanvasModel::press(const QString& sessionId, const BrowserKey& key, const QString& typed, bool down, int modifiers)
{
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    QString text;
    if (down && key.code == QStringLiteral("Space"))
        text = QStringLiteral(" ");
    else if (down && key.named == QStringLiteral("Enter"))
        text = QStringLiteral("\r");
    else if (down && key.named.isEmpty() && !command)
        text = typed;
    QJsonObject event { { QStringLiteral("type"), QStringLiteral("key") },
        { QStringLiteral("eventType"),
            !down ? QStringLiteral("keyUp") : text.isEmpty() ? QStringLiteral("rawKeyDown") : QStringLiteral("keyDown") },
        { QStringLiteral("key"), key.named.isEmpty() ? typed : key.named }, { QStringLiteral("code"), key.code },
        { QStringLiteral("keyCode"), key.windows }, { QStringLiteral("modifiers"), browserModifiers(modifiers) } };
    if (!text.isEmpty())
        event.insert(QStringLiteral("text"), text);
    send(sessionId, event);
}

void CanvasModel::type(const QString& sessionId, const QString& text)
{
    if (!text.isEmpty())
        send(sessionId, { { QStringLiteral("type"), QStringLiteral("text") }, { QStringLiteral("text"), text.left(mostTyped) } });
}

void CanvasModel::paste(const QString& sessionId)
{
    type(sessionId, QGuiApplication::clipboard()->text());
}

void CanvasModel::send(const QString& sessionId, QJsonObject input)
{
    auto& feed = m_feeds[sessionId];
    const auto moved = [](const QJsonObject& event) {
        return event.value(QStringLiteral("eventType")).toString() == QStringLiteral("mouseMoved");
    };
    if (moved(input) && !feed.pending.isEmpty() && moved(feed.pending.last()))
        feed.pending.last() = std::move(input);
    else
        feed.pending.append(std::move(input));
    if (!feed.sending)
        flush(sessionId);
}

void CanvasModel::flush(const QString& sessionId)
{
    auto& feed = m_feeds[sessionId];
    while (!feed.pending.isEmpty()) {
        QJsonArray events;
        while (!feed.pending.isEmpty() && events.size() < mostEvents)
            events.append(feed.pending.takeFirst());
        const auto error = ask(Asked::Input, sessionId,
            { { QStringLiteral("type"), QStringLiteral("input") }, { QStringLiteral("events"), events },
                { QStringLiteral("width"), qRound(feed.view.width()) },
                { QStringLiteral("height"), qRound(feed.view.height()) } });
        if (error.isEmpty()) {
            feed.sending = true;
            return;
        }
        failBrowser(sessionId, error);
    }
    feed.sending = false;
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
    case Asked::Picture:
        if (!failed) {
            receivePicture(request.sessionId, answer, request.sent);
            return;
        }
        if (const auto feed = m_feeds.find(request.sessionId); feed != m_feeds.end()) {
            feed->asking = false;
            failBrowser(request.sessionId, message);
            retryPicture(request.sessionId, request.sent);
        }
        return;
    case Asked::Input:
        if (const auto feed = m_feeds.find(request.sessionId); feed != m_feeds.end()) {
            feed->sending = false;
            if (failed)
                failBrowser(request.sessionId, message);
            flush(request.sessionId);
        }
        return;
    case Asked::Act:
        if (failed && m_opening.remove(request.sessionId)) {
            emit problem(message);
            touch();
        } else if (failed)
            failBrowser(request.sessionId, message);
        else if (const auto feed = m_feeds.find(request.sessionId); feed != m_feeds.end() && !feed->failure.isEmpty()) {
            feed->failure.clear();
            emit pictureChanged(request.sessionId);
        }
        return;
    case Asked::Remove:
        if (failed)
            emit problem(message);
        return;
    }
}

void CanvasModel::reset()
{
    m_marks.clear();
    m_firstSeen.clear();
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
    m_feeds.clear();
    m_published.clear();
    touch();
}
}
