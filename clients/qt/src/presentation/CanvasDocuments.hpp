#pragma once

#include <QByteArray>
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QString>

namespace kodosi {
class CommandDispatcher;
class SessionCatalogModel;

// The runtime keeps the pipe of a view and opens it again after each loss, until the view closes
// it.
class CanvasDocuments final : public QObject {
    Q_OBJECT

public:
    CanvasDocuments(CommandDispatcher& commands, SessionCatalogModel& sessions, QObject* parent = nullptr);

    Q_INVOKABLE [[nodiscard]] QString open(const QString& sessionId, const QString& key);
    [[nodiscard]] QString openBytes(const QString& sessionId, const QString& key);
    Q_INVOKABLE void send(const QString& pipe, const QString& frame);
    Q_INVOKABLE void close(const QString& pipe);
    void apply(const QJsonObject& event);
    void reset();

signals:
    void opened(const QString& pipe);
    void received(const QString& pipe, const QString& frame);
    void receivedBytes(const QString& pipe, const QByteArray& frame);
    void lost(const QString& pipe, const QString& reason);
    void closed(const QString& pipe);

private:
    struct Pipe {
        QString sessionId;
        QString key;
        QByteArray filling;
        // The next part, or -1 while the pipe has not opened.
        qint64 next = -1;
        bool bytes = false;
        bool registered = false;
    };

    QString openPipe(const QString& sessionId, const QString& key, bool bytes);
    bool command(const QString& type, QJsonObject fields, const QString& requestId = {});
    void registerPipe(const QString& pipe, Pipe& open);
    void renew(const QString& pipe);
    void observe();

    CommandDispatcher& m_commands;
    SessionCatalogModel& m_sessions;
    QHash<QString, Pipe> m_pipes;
    QHash<QString, QString> m_opening;
};
}
