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

// A frame comes in parts, in their order from the moment that its pipe opens, so a part that goes
// missing breaks each frame until the pipe opens again.
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
    // The next part, or -1 while the pipe has not opened.
    qint64 m_next = -1;
};

// The runtime takes a frame in parts, and a part of another frame between them breaks both.
[[nodiscard]] bool sendPipeFrame(CommandDispatcher& commands, const QString& pipe, const QString& frame);

// The runtime keeps the pipe of a view and opens it again after each loss, until the view closes
// it.
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
