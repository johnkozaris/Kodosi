#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QByteArray>
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include <optional>

namespace kodosi {
class SessionCatalogModel;

// Reassembles sequenced pipe frame chunks; any missing or invalid chunk breaks framing until the
// pipe reopens.
class PipeParts {
public:
    struct Taken {
        bool broken = false;
        std::optional<QByteArray> frame;
    };

    void open();
    void close();
    void lose();
    [[nodiscard]] Taken take(const QJsonObject& part);

private:
    QByteArray m_filling;
    // Expected sequence number, or -1 when closed.
    qint64 m_next = -1;
};

// Sends a frame in chunks; callers must serialize sends per pipe so chunks from different frames
// do not interleave.
[[nodiscard]] bool sendPipeFrame(CommandDispatcher& commands, const QString& pipe, const QString& frame);

// Manages document pipes for views; the runtime reopens them automatically after disconnects until
// they are closed.
class CanvasDocuments final : public QObject {
    Q_OBJECT

public:
    CanvasDocuments(
        CommandDispatcher& commands, PipeRouter& router, SessionCatalogModel& sessions, QObject* parent = nullptr);
    ~CanvasDocuments() override;

    Q_INVOKABLE [[nodiscard]] QString open(const QString& sessionId, const QString& key);
    [[nodiscard]] QString openReader(
        const QString& sessionId, const QString& key, QObject& reader, PipeRouter::Read read);
    [[nodiscard]] CommandDispatcher& commands() const { return m_commands; }
    Q_INVOKABLE void send(const QString& pipe, const QString& frame);
    Q_INVOKABLE void close(const QString& pipe);
    void forget(const QString& pipe);
    void renew(const QString& pipe);
    void apply(const QJsonObject& event);
    void renewPipes();
    void reset();

signals:
    void opened(const QString& pipe);
    void received(const QString& pipe, const QString& frame);
    void lost(const QString& pipe, const QString& reason);
    void closed(const QString& pipe);

private:
    struct Pipe {
        QString sessionId;
        QString key;
        PipeParts parts;
        bool routed = false;
        bool registered = false;
    };

    QString openPipe(const QString& pipe, const QString& sessionId, const QString& key, bool routed);
    bool command(const QString& type, QJsonObject fields, const QString& requestId = {});
    void registerPipe(const QString& pipe, Pipe& open);
    void remove(const QString& pipe);
    void observe();

    CommandDispatcher& m_commands;
    PipeRouter& m_router;
    SessionCatalogModel& m_sessions;
    QHash<QString, Pipe> m_pipes;
    QHash<QString, QString> m_opening;
};
}
