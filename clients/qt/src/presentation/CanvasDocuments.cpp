#include "presentation/CanvasDocuments.hpp"
#include "presentation/SessionCatalogModel.hpp"

#include <QJsonDocument>
#include <QSet>
#include <QUuid>

#include <algorithm>
#include <utility>

namespace kodosi {
namespace {
// Base64 length for ~256 KiB, rounded down to a 4-character Base64 boundary.
constexpr qsizetype partLetters = 256 * 1024 / 3 * 4;

QString newId()
{
    return QUuid::createUuidV7().toString(QUuid::WithoutBraces);
}
}

void PipeParts::open()
{
    m_filling.clear();
    m_next = 0;
}

void PipeParts::close()
{
    m_filling.clear();
    m_next = -1;
}

void PipeParts::lose()
{
    m_filling.clear();
}

PipeParts::Taken PipeParts::take(const QJsonObject& part)
{
    if (m_next < 0)
        return {};
    const auto data = QByteArray::fromBase64Encoding(part.value(QStringLiteral("data")).toString().toLatin1(),
        QByteArray::AbortOnBase64DecodingErrors);
    if (!data || part.value(QStringLiteral("sequence")).toInteger() != m_next) {
        close();
        return { .broken = true, .frame = std::nullopt };
    }
    ++m_next;
    m_filling += *data;
    if (part.value(QStringLiteral("more")).toBool())
        return {};
    return { .broken = false, .frame = std::exchange(m_filling, {}) };
}

bool sendPipeFrame(CommandDispatcher& commands, const QString& pipe, const QString& frame)
{
    for (qsizetype at = 0; at < frame.size(); at += partLetters) {
        const QJsonObject fields { { QStringLiteral("type"), QStringLiteral("document.send") },
            { QStringLiteral("requestId"), newId() }, { QStringLiteral("pipe"), pipe },
            { QStringLiteral("more"), at + partLetters < frame.size() },
            { QStringLiteral("data"), frame.mid(at, partLetters) } };
        if (!commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)))
            return false;
    }
    return true;
}

CanvasDocuments::CanvasDocuments(
    CommandDispatcher& commands, VideoWatcher& videos, SessionCatalogModel& sessions, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_videos(videos)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &CanvasDocuments::observe);
}

CanvasDocuments::~CanvasDocuments()
{
    for (auto open = m_pipes.begin(); open != m_pipes.end(); ++open)
        unwatch(open.key(), *open);
}

bool CanvasDocuments::command(const QString& type, QJsonObject fields, const QString& requestId)
{
    fields.insert(QStringLiteral("type"), type);
    fields.insert(QStringLiteral("requestId"), requestId.isEmpty() ? newId() : requestId);
    return m_commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)).has_value();
}

QString CanvasDocuments::open(const QString& sessionId, const QString& key)
{
    return openPipe(sessionId, key, {});
}

// Each terminal browser allows only one active viewer pipe per client, so two overlapping pipes
// would repeatedly evict each other. QML Loaders destroy the previous item asynchronously, so
// close any existing video pipe when a new view opens.
QString CanvasDocuments::openVideo(const QString& sessionId, const QString& key, std::shared_ptr<VideoSink> sink)
{
    const auto held = std::find_if(m_pipes.cbegin(), m_pipes.cend(),
        [&](const Pipe& open) { return open.video && open.sessionId == sessionId && open.key == key; });
    if (held != m_pipes.cend()) {
        const auto taken = held.key();
        close(taken);
        emit closed(taken, {});
    }
    return openPipe(sessionId, key, std::move(sink));
}

QString CanvasDocuments::openPipe(const QString& sessionId, const QString& key, std::shared_ptr<VideoSink> video)
{
    const auto pipe = newId();
    auto& open = *m_pipes.insert(pipe,
        { .sessionId = sessionId, .key = key, .parts = {}, .video = std::move(video), .watched = false, .registered = false });
    registerPipe(pipe, open);
    if (open.registered)
        return pipe;
    remove(pipe);
    return {};
}

void CanvasDocuments::registerPipe(const QString& pipe, Pipe& open)
{
    const auto request = newId();
    const QJsonObject fields { { QStringLiteral("sessionId"), open.sessionId }, { QStringLiteral("pipe"), pipe },
        { QStringLiteral("key"), open.key } };
    open.parts.close();
    open.registered = command(QStringLiteral("document.open"), fields, request);
    if (open.registered)
        m_opening.insert(request, pipe);
}

void CanvasDocuments::send(const QString& pipe, const QString& frame)
{
    if (m_pipes.contains(pipe) && !sendPipeFrame(m_commands, pipe, frame))
        renew(pipe);
}

void CanvasDocuments::close(const QString& pipe)
{
    if (!m_pipes.contains(pipe))
        return;
    remove(pipe);
    command(QStringLiteral("document.close"), { { QStringLiteral("pipe"), pipe } });
}

void CanvasDocuments::unwatch(const QString& pipe, Pipe& open)
{
    if (std::exchange(open.watched, false))
        m_videos.unwatch(pipe);
}

void CanvasDocuments::remove(const QString& pipe)
{
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    unwatch(pipe, *found);
    m_pipes.erase(found);
}

void CanvasDocuments::apply(const QJsonObject& event)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("document.result") || type == QStringLiteral("document.error")) {
        const auto pipe = m_opening.take(event.value(QStringLiteral("requestId")).toString());
        const auto found = m_pipes.find(pipe);
        if (found == m_pipes.end())
            return;
        if (type == QStringLiteral("document.error")) {
            remove(pipe);
            emit closed(pipe, event.value(QStringLiteral("message")).toString());
        } else if (found->video && !std::exchange(found->watched, true)) {
            m_videos.watch(pipe, found->video);
        }
        return;
    }
    const auto pipe = event.value(QStringLiteral("pipe")).toString();
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    if (type == QStringLiteral("document.opened")) {
        found->parts.open();
        emit opened(pipe);
        return;
    }
    if (type == QStringLiteral("document.lost")) {
        found->parts.lose();
        emit lost(pipe, event.value(QStringLiteral("message")).toString());
        return;
    }
    if (type == QStringLiteral("document.closed")) {
        remove(pipe);
        emit closed(pipe, {});
        return;
    }
    if (type != QStringLiteral("document.frame"))
        return;
    const auto taken = found->parts.take(event);
    if (taken.broken)
        renew(pipe);
    else if (taken.frame)
        emit received(pipe, QString::fromLatin1(taken.frame->toBase64()));
}

// A restarted host has no open pipes, so mark them unregistered to reopen once their sessions are
// listed.
void CanvasDocuments::reset()
{
    m_opening.clear();
    for (auto open = m_pipes.begin(); open != m_pipes.end(); ++open) {
        unwatch(open.key(), *open);
        open->parts.close();
        open->registered = false;
    }
    const auto pipes = m_pipes.keys();
    for (const auto& pipe : pipes)
        emit lost(pipe, {});
}

// Dropped runtime events may include pipe chunks or open replies, so reopen registered pipes.
void CanvasDocuments::renewPipes()
{
    m_opening.clear();
    for (const auto& [pipe, open] : m_pipes.asKeyValueRange()) {
        if (open.registered)
            renew(pipe);
    }
}

// What the pipe carried lost a part, so close and reopen it; parts still on their way are ignored
// until the reopened pipe arrives.
void CanvasDocuments::renew(const QString& pipe)
{
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    unwatch(pipe, *found);
    command(QStringLiteral("document.close"), { { QStringLiteral("pipe"), pipe } });
    registerPipe(pipe, *found);
}

void CanvasDocuments::observe()
{
    QSet<QString> listed;
    for (const auto& value : m_sessions.sessions())
        listed.insert(value.toMap().value(QStringLiteral("id")).toString());
    for (auto open = m_pipes.begin(); open != m_pipes.end(); ++open) {
        if (!open->registered && listed.contains(open->sessionId))
            registerPipe(open.key(), *open);
    }
}
}
