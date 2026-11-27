#pragma once

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

#include <optional>

namespace kodosi {
class CanvasDocuments;
class SessionCatalogModel;

// Models a session's agent chat stream. The document pipe stays open only while watched, while
// the instance persists with its draft for the lifetime of the session.
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
    Q_PROPERTY(QVariantList helpers READ helpers NOTIFY helpersChanged FINAL)
    Q_PROPERTY(QVariantMap facts READ facts NOTIFY factsChanged FINAL)
    Q_PROPERTY(QVariantList commands READ commands NOTIFY commandsChanged FINAL)
    Q_PROPERTY(QVariantList settings READ settings NOTIFY settingsChanged FINAL)
    Q_PROPERTY(QVariantList peers READ peers NOTIFY peersChanged FINAL)
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
    // True when the agent has an ask not yet answered locally.
    [[nodiscard]] bool asking() const;
    [[nodiscard]] QVariantMap answers() const;
    [[nodiscard]] QVariantList queued() const { return m_queued; }
    [[nodiscard]] QVariantList tasks() const { return m_tasks; }
    // The helpers that work now, in the order that the agent started them. A helper that goes on
    // after its turn is here until it ends.
    [[nodiscard]] QVariantList helpers() const { return m_helpers; }
    [[nodiscard]] QVariantMap facts() const { return m_facts; }
    [[nodiscard]] QVariantList commands() const { return m_commands; }
    [[nodiscard]] QVariantList settings() const { return m_settings; }
    // The agents of other terminals that this terminal's agent can reach, in the order to show.
    [[nodiscard]] QVariantList peers() const { return m_peers; }
    [[nodiscard]] QString draft() const { return m_draft; }
    void setDraft(const QString& draft);
    [[nodiscard]] QStringList opened() const { return m_opened; }

    // Reference-counted subscription; multiple views share a single pipe.
    Q_INVOKABLE void watch();
    Q_INVOKABLE void unwatch();
    // Sends the draft; if an ask is pending, submits the draft as a text denial instead. Returns
    // true if sent.
    Q_INVOKABLE bool submit();
    // Expects `kind`: "allow", "always", "deny" (optional `text`), "answers" (`answers` label
    // lists), or "plan" (`choice`).
    Q_INVOKABLE void answer(const QString& ask, const QVariantMap& answer);
    Q_INVOKABLE void unqueue(const QString& id);
    // Asks the agent to go on with the work of the agent of a peer's terminal, which gets no message.
    Q_INVOKABLE void handOff(const QString& terminal);
    Q_INVOKABLE void stop();
    // Asks the agent for its own settings, which arrive in `settings`.
    Q_INVOKABLE void askSettings();
    // Changes one of the agent's settings to `value`, in the words of the setting's value.
    Q_INVOKABLE void set(const QString& key, const QString& value);
    Q_INVOKABLE void toggle(const QString& id);
    // The control of the chat that a draft of only a command's name opens in place of sending it.
    Q_INVOKABLE [[nodiscard]] QString control(const QString& draft) const;
    // A link in the chat opens by the rules of a link in the terminal. A file of a terminal on
    // another computer is on that computer.
    Q_INVOKABLE void follow(const QString& link, const QString& otherComputer);

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
    void helpersChanged();
    void factsChanged();
    void commandsChanged();
    void settingsChanged();
    void peersChanged();
    void draftChanged();
    void openedChanged();
    void linkQuestionRequested(const QUrl& link, const QString& target);
    void linkRefused(const QString& target, const QString& reason);

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
        QJsonArray commands;
        QJsonArray settings;
        QJsonArray peers;
    };
    struct Prose {
        QString text;
        QString folder;
        QVariantList blocks;
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
    [[nodiscard]] QString question() const;

    QPointer<CanvasDocuments> m_documents;
    QString m_sessionId;
    QString m_pipe;
    int m_watchers = 0;
    std::optional<Snapshot> m_chat;
    // Ignore incremental updates after a pipe opens until the host sends the initial snapshot.
    bool m_awaitsSnapshot = true;
    bool m_linked = false;
    QString m_failure;
    // Locally submitted answers kept until the host clears the ask or sends a fresh snapshot.
    QHash<QString, QJsonObject> m_answers;
    QString m_draft;
    QStringList m_opened;
    QHash<QString, Prose> m_prose;
    QVariantList m_lines;
    bool m_empty = false;
    QVariantMap m_ask;
    QVariantList m_queued;
    QVariantList m_tasks;
    QVariantList m_helpers;
    QVariantMap m_facts;
    QVariantList m_commands;
    QVariantList m_settings;
    QVariantList m_peers;
};

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
