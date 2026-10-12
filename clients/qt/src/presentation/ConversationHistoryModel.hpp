#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

#include <optional>

namespace kodosi {
class ConversationHistoryModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(Provider provider READ provider NOTIFY selectionChanged)
    Q_PROPERTY(QString providerId READ providerId NOTIFY selectionChanged)
    Q_PROPERTY(QString directory READ directory NOTIFY selectionChanged)
    Q_PROPERTY(QVariantList conversations READ conversations NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantList entries READ entries NOTIFY entriesChanged)
    Q_PROPERTY(QVariantMap selectedConversation READ selectedConversation NOTIFY entriesChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool hasMore READ hasMore NOTIFY conversationsChanged)
    Q_PROPERTY(bool hasOlder READ hasOlder NOTIFY entriesChanged)
    Q_PROPERTY(bool hasNewer READ hasNewer NOTIFY entriesChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)

public:
    enum Provider { Claude, Copilot };
    Q_ENUM(Provider)

    explicit ConversationHistoryModel(CommandDispatcher& commands, QObject* parent = nullptr);

    Provider provider() const { return m_provider; }
    QString providerId() const;
    QString directory() const { return m_directory; }
    QVariantList conversations() const { return m_conversations; }
    QVariantList entries() const { return m_entries; }
    QVariantMap selectedConversation() const { return m_selected; }
    bool busy() const { return !m_requestId.isEmpty(); }
    bool hasMore() const { return !m_cursor.isEmpty(); }
    bool hasOlder() const { return m_before.has_value(); }
    bool hasNewer() const { return !m_newer.isEmpty(); }
    QString error() const { return m_error; }

    Q_INVOKABLE void open(Provider provider, const QString& directory);
    Q_INVOKABLE void loadMore();
    Q_INVOKABLE void preview(const QString& nativeId);
    Q_INVOKABLE void loadOlder();
    Q_INVOKABLE void loadNewer();
    Q_INVOKABLE void loadLatest();
    Q_INVOKABLE void reset();

    void apply(const QJsonObject& event, const QByteArray& payload);

signals:
    void selectionChanged();
    void conversationsChanged();
    void entriesChanged();
    void busyChanged();
    void errorChanged();

private:
    enum class Navigation { Latest, Older, Newer };

    CommandDispatcher& m_commands;
    QTimer m_timeout;
    Provider m_provider = Claude;
    QString m_directory;
    QString m_requestId;
    QString m_operation;
    QString m_error;
    QString m_cursor;
    QVariantList m_conversations;
    QVariantList m_entries;
    QVariantMap m_selected;
    std::optional<std::uint64_t> m_before;
    std::optional<std::uint64_t> m_pageBefore;
    std::optional<std::uint64_t> m_requestedBefore;
    QList<std::optional<std::uint64_t>> m_newer;
    Navigation m_navigation = Navigation::Latest;
    bool m_append = false;

    void discover();
    void readPage(std::optional<std::uint64_t> before, Navigation navigation);
    void send(QString operation, QJsonObject values = {});
    void clearError();
    void fail(QString message);
};
}
