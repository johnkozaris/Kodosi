#include "presentation/CanvasDocuments.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QJsonDocument>
#include <QSet>
#include <QUuid>

#include <utility>

namespace kodosi {
namespace {
// Base64 of 256 KiB, cut at a whole group of four letters.
constexpr qsizetype partLetters = 256 * 1024 / 3 * 4;

QString newId()
{
    return QUuid::createUuidV7().toString(QUuid::WithoutBraces);
}
}

CanvasDocuments::CanvasDocuments(CommandDispatcher& commands, SessionCatalogModel& sessions, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_sessions(sessions)
{
    connect(&sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, &CanvasDocuments::observe);
}

bool CanvasDocuments::command(const QString& type, QJsonObject fields, const QString& requestId)
{
    fields.insert(QStringLiteral("type"), type);
    fields.insert(QStringLiteral("requestId"), requestId.isEmpty() ? newId() : requestId);
    return m_commands.send(QJsonDocument(fields).toJson(QJsonDocument::Compact)).has_value();
}

QString CanvasDocuments::open(const QString& sessionId, const QString& key)
{
    return openPipe(sessionId, key, false);
}

QString CanvasDocuments::openBytes(const QString& sessionId, const QString& key)
{
    return openPipe(sessionId, key, true);
}

QString CanvasDocuments::openPipe(const QString& sessionId, const QString& key, bool bytes)
{
    const auto pipe = newId();
    auto& open = *m_pipes.insert(pipe,
        { .sessionId = sessionId, .key = key, .filling = {}, .next = -1, .bytes = bytes, .registered = false });
    registerPipe(pipe, open);
    if (open.registered)
        return pipe;
    m_pipes.remove(pipe);
    return {};
}

void CanvasDocuments::registerPipe(const QString& pipe, Pipe& open)
{
    const auto request = newId();
    const QJsonObject fields { { QStringLiteral("sessionId"), open.sessionId }, { QStringLiteral("pipe"), pipe },
        { QStringLiteral("key"), open.key } };
    open.filling.clear();
    open.next = -1;
    open.registered = command(QStringLiteral("document.open"), fields, request);
    if (open.registered)
        m_opening.insert(request, pipe);
}

void CanvasDocuments::send(const QString& pipe, const QString& frame)
{
    if (!m_pipes.contains(pipe))
        return;
    for (qsizetype at = 0; at < frame.size(); at += partLetters) {
        const QJsonObject fields { { QStringLiteral("pipe"), pipe },
            { QStringLiteral("more"), at + partLetters < frame.size() },
            { QStringLiteral("data"), frame.mid(at, partLetters) } };
        if (!command(QStringLiteral("document.send"), fields)) {
            renew(pipe);
            return;
        }
    }
}

void CanvasDocuments::close(const QString& pipe)
{
    if (m_pipes.remove(pipe) > 0)
        command(QStringLiteral("document.close"), { { QStringLiteral("pipe"), pipe } });
}

void CanvasDocuments::apply(const QJsonObject& event)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("document.result") || type == QStringLiteral("document.error")) {
        const auto pipe = m_opening.take(event.value(QStringLiteral("requestId")).toString());
        if (type == QStringLiteral("document.error") && m_pipes.remove(pipe) > 0)
            emit closed(pipe);
        return;
    }
    const auto pipe = event.value(QStringLiteral("pipe")).toString();
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    if (type == QStringLiteral("document.opened")) {
        found->filling.clear();
        found->next = 0;
        emit opened(pipe);
        return;
    }
    if (type == QStringLiteral("document.lost")) {
        found->filling.clear();
        emit lost(pipe, event.value(QStringLiteral("message")).toString());
        return;
    }
    if (type == QStringLiteral("document.closed")) {
        m_pipes.erase(found);
        emit closed(pipe);
        return;
    }
    if (type != QStringLiteral("document.frame") || found->next < 0)
        return;
    const auto part = QByteArray::fromBase64Encoding(event.value(QStringLiteral("data")).toString().toLatin1(),
        QByteArray::AbortOnBase64DecodingErrors);
    if (!part || event.value(QStringLiteral("sequence")).toInteger() != found->next) {
        renew(pipe);
        return;
    }
    ++found->next;
    found->filling += *part;
    if (event.value(QStringLiteral("more")).toBool())
        return;
    const auto frame = std::exchange(found->filling, {});
    if (found->bytes)
        emit receivedBytes(pipe, frame);
    else
        emit received(pipe, QString::fromLatin1(frame.toBase64()));
}

// The host that comes back knows no pipe, so each one goes to it again once it lists the terminal.
void CanvasDocuments::reset()
{
    m_opening.clear();
    for (auto& open : m_pipes) {
        open.filling.clear();
        open.next = -1;
        open.registered = false;
    }
    const auto pipes = m_pipes.keys();
    for (const auto& pipe : pipes)
        emit lost(pipe, {});
}

// A part that went missing breaks the frames, so the pipe starts again, and the parts that were on
// their way go nowhere.
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
