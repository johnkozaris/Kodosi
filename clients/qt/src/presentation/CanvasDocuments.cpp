#include "presentation/CanvasDocuments.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QJsonDocument>
#include <QStringList>
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
    const auto pipe = newId();
    const auto request = newId();
    m_pipes.insert(pipe, {});
    m_opening.insert(request, pipe);
    const QJsonObject fields { { QStringLiteral("sessionId"), sessionId }, { QStringLiteral("pipe"), pipe },
        { QStringLiteral("key"), key } };
    if (command(QStringLiteral("document.open"), fields, request))
        return pipe;
    m_pipes.remove(pipe);
    m_opening.remove(request);
    return {};
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
            lose(pipe);
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
        if (!m_pipes.contains(pipe))
            return;
        if (type == QStringLiteral("document.result"))
            emit opened(pipe);
        else
            lose(pipe);
        return;
    }
    const auto pipe = event.value(QStringLiteral("pipe")).toString();
    const auto found = m_pipes.find(pipe);
    if (found == m_pipes.end())
        return;
    const auto part = QByteArray::fromBase64Encoding(event.value(QStringLiteral("data")).toString().toLatin1(),
        QByteArray::AbortOnBase64DecodingErrors);
    if (type != QStringLiteral("document.frame") || !part
        || event.value(QStringLiteral("sequence")).toInteger() != found->next) {
        lose(pipe);
        return;
    }
    ++found->next;
    found->filling += *part;
    if (!event.value(QStringLiteral("more")).toBool())
        emit received(pipe, QString::fromLatin1(std::exchange(found->filling, {}).toBase64()));
}

void CanvasDocuments::reset()
{
    const auto pipes = m_pipes.keys();
    for (const auto& pipe : pipes)
        lose(pipe);
    m_opening.clear();
    m_running.clear();
}

// The runtime keeps a pipe until its client closes it, so a broken pipe closes there too.
void CanvasDocuments::lose(const QString& pipe)
{
    close(pipe);
    emit lost(pipe);
}

void CanvasDocuments::observe()
{
    QSet<QString> running;
    for (const auto& value : m_sessions.sessions()) {
        const auto session = value.toMap();
        if (session.value(QStringLiteral("status")).toString() == QStringLiteral("running"))
            running.insert(session.value(QStringLiteral("id")).toString());
    }
    for (const auto& id : std::as_const(running)) {
        if (!m_running.contains(id))
            emit reachable(id);
    }
    m_running = std::move(running);
}
}
