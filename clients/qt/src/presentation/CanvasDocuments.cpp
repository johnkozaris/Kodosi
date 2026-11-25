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
    CommandDispatcher& commands, PipeRouter& router, SessionCatalogModel& sessions, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_router(router)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &CanvasDocuments::observe);
}

CanvasDocuments::~CanvasDocuments()
{
    for (const auto& [pipe, open] : m_pipes.asKeyValueRange()) {
        if (open.routed)
            m_router.unroute(pipe);
    }
}

bool CanvasDocuments::command(const QString& type, QJsonObject fields, const QString& requestId)
{
    fields.insert(QStringLiteral("type"), type);
    fields.insert(QStringLiteral("requestId"), requestId.isEmpty() ? newId() : requestId);
    return m_commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)).has_value();
}

QString CanvasDocuments::open(const QString& sessionId, const QString& key)
{
    return openPipe(newId(), sessionId, key, false);
}

// Each terminal browser allows only one active viewer pipe per client, so two overlapping pipes
// would repeatedly evict each other. QML Loaders destroy the previous item asynchronously, so
// close any existing routed pipe when a new reader opens.
QString CanvasDocuments::openReader(
    const QString& sessionId, const QString& key, QObject& reader, PipeRouter::Read read)
{
    const auto held = std::find_if(m_pipes.cbegin(), m_pipes.cend(),
        [&](const Pipe& open) { return open.routed && open.sessionId == sessionId && open.key == key; });
    if (held != m_pipes.cend()) {
        const auto taken = held.key();
        close(taken);
        emit closed(taken);
    }
    const auto pipe = newId();
    m_router.route(pipe, reader, std::move(read));
    return openPipe(pipe, sessionId, key, true);
}

QString CanvasDocuments::openPipe(const QString& pipe, const QString& sessionId, const QString& key, bool routed)
{
    auto& open = *m_pipes.insert(
        pipe, { .sessionId = sessionId, .key = key, .parts = {}, .routed = routed, .registered = false });
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

// Remove local state for a reader pipe already closed by the host.
void CanvasDocuments::forget(const QString& pipe)
{
    remove(pipe);
}

void CanvasDocuments::remove(const QString& pipe)
{
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    if (found->routed)
        m_router.unroute(pipe);
    m_pipes.erase(found);
}

void CanvasDocuments::apply(const QJsonObject& event)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("document.result") || type == QStringLiteral("document.error")) {
        const auto pipe = m_opening.take(event.value(QStringLiteral("requestId")).toString());
        if (type == QStringLiteral("document.error") && m_pipes.contains(pipe)) {
            remove(pipe);
            emit closed(pipe);
        }
        return;
    }
    const auto pipe = event.value(QStringLiteral("pipe")).toString();
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end() || found->routed)
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
        m_pipes.erase(found);
        emit closed(pipe);
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
    for (auto& open : m_pipes) {
        open.parts.close();
        open.registered = false;
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

// A missing chunk corrupts framing, so close and reopen the pipe; in-flight chunks are ignored
// until the reopened pipe arrives.
void CanvasDocuments::renew(const QString& pipe)
{
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
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
