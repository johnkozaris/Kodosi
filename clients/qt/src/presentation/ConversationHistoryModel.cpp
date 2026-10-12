#include "presentation/ConversationHistoryModel.hpp"
#include "runtime/JsonEnvelope.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QUuid>

namespace kodosi {
ConversationHistoryModel::ConversationHistoryModel(CommandDispatcher& commands, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
{
    m_timeout.setSingleShot(true);
    m_timeout.setInterval(15000);
    connect(&m_timeout, &QTimer::timeout, this, [this] {
        m_requestId.clear();
        emit busyChanged();
        fail(tr("The provider did not respond in time."));
    });
}

QString ConversationHistoryModel::providerId() const
{
    return m_provider == Claude ? QStringLiteral("claude") : QStringLiteral("copilot");
}

void ConversationHistoryModel::open(Provider provider, const QString& directory)
{
    const bool changed = m_provider != provider || m_directory != directory;
    reset();
    m_provider = provider;
    m_directory = directory;
    if (changed)
        emit selectionChanged();
    discover();
}

void ConversationHistoryModel::reset()
{
    m_timeout.stop();
    m_requestId.clear();
    m_operation.clear();
    m_conversations.clear();
    m_entries.clear();
    m_selected.clear();
    m_cursor.clear();
    m_before.reset();
    m_pageBefore.reset();
    m_requestedBefore.reset();
    m_newer.clear();
    m_error.clear();
    emit busyChanged();
    emit conversationsChanged();
    emit entriesChanged();
    emit errorChanged();
}

void ConversationHistoryModel::send(QString operation, QJsonObject values)
{
    m_timeout.stop();
    m_requestId = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    m_operation = operation;
    values.insert(QStringLiteral("type"), operation);
    values.insert(QStringLiteral("requestId"), m_requestId);
    values.insert(QStringLiteral("provider"), providerId());
    if (!values.contains(QStringLiteral("workingDirectory")) && !m_directory.isEmpty())
        values.insert(QStringLiteral("workingDirectory"), m_directory);
    auto bytes = QJsonDocument(values).toJson(QJsonDocument::Compact);
    if (operation == QStringLiteral("provider.readConversation") && m_requestedBefore) {
        bytes.chop(1);
        bytes += QByteArrayLiteral(",\"beforeByte\":")
            + QByteArray::number(*m_requestedBefore) + '}';
    }
    const auto result = m_commands.send(bytes);
    if (!result) {
        m_requestId.clear();
        fail(result.error().message);
    } else {
        m_timeout.start();
        clearError();
    }
    emit busyChanged();
}

void ConversationHistoryModel::discover()
{
    m_conversations.clear();
    m_cursor.clear();
    m_selected.clear();
    m_entries.clear();
    m_before.reset();
    m_pageBefore.reset();
    m_newer.clear();
    m_append = false;
    emit conversationsChanged();
    emit entriesChanged();
    send(QStringLiteral("provider.discoverConversations"),
        { { QStringLiteral("limit"), 50 }, { QStringLiteral("maxBytes"), 262144 } });
}

void ConversationHistoryModel::loadMore()
{
    if (busy() || m_cursor.isEmpty())
        return;
    m_append = true;
    send(QStringLiteral("provider.discoverConversations"),
        { { QStringLiteral("cursor"), m_cursor }, { QStringLiteral("limit"), 50 },
            { QStringLiteral("maxBytes"), 262144 } });
}

void ConversationHistoryModel::preview(const QString& nativeId)
{
    QVariantMap selected;
    for (const auto& value : m_conversations) {
        const auto item = value.toMap();
        if (item.value(QStringLiteral("nativeConversationId")).toString() == nativeId) {
            selected = item;
            break;
        }
    }
    if (selected.isEmpty()) {
        fail(tr("Choose a conversation from this project."));
        return;
    }
    m_selected = selected;
    m_entries.clear();
    m_before.reset();
    m_pageBefore.reset();
    m_newer.clear();
    m_append = false;
    emit entriesChanged();
    readPage(std::nullopt, Navigation::Latest);
}

void ConversationHistoryModel::readPage(
    std::optional<std::uint64_t> before, Navigation navigation)
{
    m_requestedBefore = before;
    m_navigation = navigation;
    send(QStringLiteral("provider.readConversation"),
        { { QStringLiteral("nativeConversationId"),
              m_selected.value(QStringLiteral("nativeConversationId")).toString() },
            { QStringLiteral("workingDirectory"),
                m_selected.value(QStringLiteral("workingDirectory")).toString() },
            { QStringLiteral("limit"), 50 }, { QStringLiteral("maxBytes"), 131072 } });
}

void ConversationHistoryModel::loadOlder()
{
    if (busy() || !m_before || m_selected.isEmpty())
        return;
    readPage(m_before, Navigation::Older);
}

void ConversationHistoryModel::loadNewer()
{
    if (busy() || m_newer.isEmpty() || m_selected.isEmpty())
        return;
    readPage(m_newer.last(), Navigation::Newer);
}

void ConversationHistoryModel::loadLatest()
{
    if (busy() || m_selected.isEmpty())
        return;
    readPage(std::nullopt, Navigation::Latest);
}

void ConversationHistoryModel::apply(const QJsonObject& event, const QByteArray& payload)
{
    if (event.value(QStringLiteral("requestId")).toString() != m_requestId
        || m_requestId.isEmpty()) {
        return;
    }
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type != QStringLiteral("provider.reply")
        && type != QStringLiteral("provider.error")) {
        return;
    }
    m_requestId.clear();
    m_timeout.stop();
    emit busyChanged();
    if (type == QStringLiteral("provider.error")) {
        fail(event.value(QStringLiteral("message")).toString());
        return;
    }
    if (event.value(QStringLiteral("operation")).toString() != m_operation
        || !event.value(QStringLiteral("result")).isObject()) {
        fail(tr("The provider returned an unexpected response."));
        return;
    }
    const auto result = event.value(QStringLiteral("result")).toObject();
    if (m_operation == QStringLiteral("provider.discoverConversations")) {
        const auto items = result.value(QStringLiteral("items")).toArray();
        if (items.size() > 100 || m_conversations.size() + items.size() > 1000) {
            fail(tr("The conversation list reached its display limit."));
            return;
        }
        QVariantList accepted;
        for (const auto& value : items) {
            const auto item = value.toObject();
            if (item.value(QStringLiteral("provider")).toString() != providerId()
                || (!m_directory.isEmpty()
                    && item.value(QStringLiteral("workingDirectory")).toString()
                        != m_directory)
                || QUuid(item.value(QStringLiteral("nativeConversationId")).toString())
                       .isNull()) {
                fail(tr("A conversation did not match this project."));
                return;
            }
            accepted.append(item.toVariantMap());
        }
        if (m_append)
            m_conversations.append(accepted);
        else
            m_conversations = accepted;
        m_cursor = result.value(QStringLiteral("nextCursor")).toString();
        emit conversationsChanged();
        return;
    }
    if (m_operation != QStringLiteral("provider.readConversation"))
        return;
    const auto entries = result.value(QStringLiteral("entries")).toArray();
    if (entries.size() > 100) {
        fail(tr("The conversation preview exceeded its page limit."));
        return;
    }
    switch (m_navigation) {
    case Navigation::Older:
        m_newer.append(m_pageBefore);
        break;
    case Navigation::Newer:
        if (!m_newer.isEmpty())
            m_newer.removeLast();
        break;
    case Navigation::Latest:
        m_newer.clear();
        break;
    }
    m_pageBefore = m_requestedBefore;
    m_entries = entries.toVariantList();
    m_before.reset();
    if (const auto fields = jsonObjectFields(payload)) {
        for (const auto& field : *fields) {
            if (field.name != QStringLiteral("result"))
                continue;
            if (const auto resultFields = jsonObjectFields(field.value.toByteArray())) {
                for (const auto& resultField : *resultFields) {
                    if (resultField.name == QStringLiteral("nextBeforeByte"))
                        m_before = jsonUnsigned(resultField.value);
                }
            }
        }
    }
    emit entriesChanged();
    const auto degraded = result.value(QStringLiteral("degradedReason")).toString();
    if (!degraded.isEmpty())
        fail(degraded);
}

void ConversationHistoryModel::clearError()
{
    if (m_error.isEmpty())
        return;
    m_error.clear();
    emit errorChanged();
}

void ConversationHistoryModel::fail(QString message)
{
    m_error = std::move(message);
    emit errorChanged();
}
}
