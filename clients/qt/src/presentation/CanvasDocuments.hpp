#pragma once

#include <QByteArray>
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QSet>
#include <QString>

namespace kodosi {
class CommandDispatcher;
class SessionCatalogModel;

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
    void lost(const QString& pipe);
    void reachable(const QString& sessionId);

private:
    struct Pipe {
        QByteArray filling;
        qint64 next = 0;
        bool bytes = false;
    };

    QString openPipe(const QString& sessionId, const QString& key, bool bytes);
    bool command(const QString& type, QJsonObject fields, const QString& requestId = {});
    void lose(const QString& pipe);
    void observe();

    CommandDispatcher& m_commands;
    SessionCatalogModel& m_sessions;
    QHash<QString, Pipe> m_pipes;
    QHash<QString, QString> m_opening;
    QSet<QString> m_running;
};
}
