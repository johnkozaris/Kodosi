#pragma once

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

#include <optional>

namespace kodosi {
class CanvasDocuments;
class SessionCatalogModel;

// The chat of one terminal, as the terminal's computer keeps it: a second view of the terminal,
// built from the reports of its agent. The pipe is open only while a view watches, and the chat
// stays with what a person writes there while the terminal does.
class TerminalChat final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString sessionId READ sessionId CONSTANT FINAL)
    Q_PROPERTY(bool loaded READ loaded NOTIFY threadChanged FINAL)
    Q_PROPERTY(QVariantList lines READ lines NOTIFY threadChanged FINAL)
    Q_PROPERTY(bool empty READ empty NOTIFY threadChanged FINAL)
    Q_PROPERTY(QString state READ state NOTIFY stateChanged FINAL)
    Q_PROPERTY(bool working READ working NOTIFY stateChanged FINAL)
    Q_PROPERTY(bool canWrite READ canWrite NOTIFY stateChanged FINAL)
    Q_PROPERTY(QString failure READ failure NOTIFY stateChanged FINAL)
    Q_PROPERTY(QVariantMap ask READ ask NOTIFY asksChanged FINAL)
    Q_PROPERTY(bool asking READ asking NOTIFY asksChanged FINAL)
    Q_PROPERTY(QVariantMap answers READ answers NOTIFY asksChanged FINAL)
    Q_PROPERTY(QVariantList queued READ queued NOTIFY queuedChanged FINAL)
    Q_PROPERTY(QVariantList tasks READ tasks NOTIFY tasksChanged FINAL)
    Q_PROPERTY(QVariantMap facts READ facts NOTIFY factsChanged FINAL)
    Q_PROPERTY(QString draft READ draft WRITE setDraft NOTIFY draftChanged FINAL)
    Q_PROPERTY(QStringList opened READ opened NOTIFY openedChanged FINAL)

public:
    TerminalChat(CanvasDocuments& documents, QString sessionId, QObject* parent = nullptr);
    ~TerminalChat() override;

    [[nodiscard]] QString sessionId() const { return m_sessionId; }
    [[nodiscard]] QString pipe() const { return m_pipe; }
    [[nodiscard]] bool watched() const { return m_watchers > 0; }
    [[nodiscard]] bool loaded() const { return m_chat.has_value(); }
    [[nodiscard]] QVariantList lines() const { return m_lines; }
    [[nodiscard]] bool empty() const { return m_empty; }
    [[nodiscard]] QString state() const;
    [[nodiscard]] bool working() const;
    [[nodiscard]] bool canWrite() const;
    [[nodiscard]] QString failure() const { return m_failure; }
    [[nodiscard]] QVariantMap ask() const { return m_ask; }
    // A question of the agent that no person on this device answered yet.
    [[nodiscard]] bool asking() const;
    [[nodiscard]] QVariantMap answers() const;
    [[nodiscard]] QVariantList queued() const { return m_queued; }
    [[nodiscard]] QVariantList tasks() const { return m_tasks; }
    [[nodiscard]] QVariantMap facts() const { return m_facts; }
    [[nodiscard]] QString draft() const { return m_draft; }
    void setDraft(const QString& draft);
    [[nodiscard]] QStringList opened() const { return m_opened; }

    // Each view that shows the chat watches until it goes, and they share one pipe.
    Q_INVOKABLE void watch();
    Q_INVOKABLE void unwatch();
    // A typed message while a question waits answers it: the agent reads the words in place of
    // what it asked to do. True when the words went.
    Q_INVOKABLE bool submit();
    // `answer` has its `kind`: allow, always, deny with an optional `text`, answers with
    // `answers` (for each question the labels that the person chose), or plan with `choice`.
    Q_INVOKABLE void answer(const QString& ask, const QVariantMap& answer);
    Q_INVOKABLE void unqueue(const QString& id);
    Q_INVOKABLE void stop();
    // The steps that a person opened stay open while the chat shows.
    Q_INVOKABLE void toggle(const QString& id);

    void pipeOpened();
    void receive(const QByteArray& frame);
    void lose(const QString& reason);
    void pipeClosed();

signals:
    void threadChanged();
    void stateChanged();
    void asksChanged();
    void queuedChanged();
    void tasksChanged();
    void factsChanged();
    void draftChanged();
    void openedChanged();

private:
    struct Snapshot {
        quint64 revision = 0;
        QString state;
        QJsonObject now;
        QJsonObject facts;
        QList<QJsonObject> items;
        QJsonArray asks;
        QJsonArray queued;
        QJsonArray tasks;
    };

    [[nodiscard]] static std::optional<Snapshot> snapshotFrom(const QJsonObject& value);
    [[nodiscard]] static bool apply(Snapshot& chat, const QJsonObject& change);
    void request(const QJsonObject& request);
    void renew();
    void shape();
    void setLinked(bool linked);
    [[nodiscard]] QVariantList blocks(const QString& id, const QString& text);
    [[nodiscard]] QVariantMap shapedAsk(const QJsonObject& ask);
    [[nodiscard]] QVariantMap shapedFacts() const;
    [[nodiscard]] QVariantMap step(const QJsonObject& item);
    [[nodiscard]] QString nowWords() const;
    // The first question that no person on this device answered yet.
    [[nodiscard]] QString question() const;

    QPointer<CanvasDocuments> m_documents;
    QString m_sessionId;
    QString m_pipe;
    int m_watchers = 0;
    std::optional<Snapshot> m_chat;
    // After each opening of the pipe the host sends the whole chat first, and the changes count
    // from it.
    bool m_awaitsSnapshot = true;
    bool m_linked = false;
    QString m_failure;
    // The answers that a person gave on this device, until their questions are gone or the chat
    // comes again.
    QHash<QString, QJsonObject> m_answers;
    QString m_draft;
    QStringList m_opened;
    QHash<QString, std::pair<QString, QVariantList>> m_prose;
    QVariantList m_lines;
    bool m_empty = false;
    QVariantMap m_ask;
    QVariantList m_queued;
    QVariantList m_tasks;
    QVariantMap m_facts;
};

// The chats of the terminals that this app shows. Each pipe of a chat goes to its chat.
class TerminalChats final : public QObject {
    Q_OBJECT

public:
    TerminalChats(CanvasDocuments& documents, SessionCatalogModel& sessions, QObject* parent = nullptr);

    Q_INVOKABLE kodosi::TerminalChat* chat(const QString& sessionId);
    Q_INVOKABLE [[nodiscard]] static QString modelName(const QString& model, const QVariantMap& models);

private:
    [[nodiscard]] TerminalChat* byPipe(const QString& pipe) const;
    void forgetClosed();

    CanvasDocuments& m_documents;
    SessionCatalogModel& m_sessions;
    QHash<QString, TerminalChat*> m_chats;
};

}
