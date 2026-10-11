#include "platform/TerminalNotifications.hpp"

#include "platform/NotificationText.hpp"

#include <QGuiApplication>
#include <QUuid>

#include <utility>

namespace kodosi {

TerminalNotifications::TerminalNotifications(
    SessionCatalogModel& sessions,
    DesktopNotificationDriver& driver,
    QObject* parent)
    : QObject(parent)
    , isFront([] { return QGuiApplication::applicationState() == Qt::ApplicationActive; })
    , m_sessions(sessions)
    , m_driver(driver)
{
    connect(
        &m_driver,
        &DesktopNotificationDriver::actionInvoked,
        this,
        [this](
            const QString& key,
            const QString& action,
            const QString& activationToken) {
            handleAction(key, action, activationToken);
        });
    connect(
        &m_driver,
        &DesktopNotificationDriver::notificationClosed,
        this,
        [this](const QString& key) {
            if (owns(key)) {
                remove(key, false);
            }
        });
    connect(
        &m_driver,
        &DesktopNotificationDriver::deliveryError,
        this,
        [this](const QString& key, const QString& message) {
            if (owns(key)) {
                emit deliveryError(message);
            }
        });
    const auto pruneContexts = [this] { prune(); };
    connect(&m_sessions, &QAbstractItemModel::modelReset, this, pruneContexts);
    connect(&m_sessions, &QAbstractItemModel::rowsRemoved, this, pruneContexts);
    connect(&m_sessions, &QAbstractItemModel::dataChanged, this, pruneContexts);
    connect(&m_sessions, &SessionCatalogModel::attentionChanged, this, pruneContexts);
    connect(&m_sessions, &SessionCatalogModel::alertChanged, this, &TerminalNotifications::alert);
}

void TerminalNotifications::alert(const QString& sessionId, const QString& text)
{
    const auto key = QStringLiteral("terminal:alert:") + sessionId;
    const auto session = m_sessions.session(sessionId);
    if (text.isEmpty() || !session || isFront()) {
        remove(key, true);
        return;
    }
    track(key, { .sessionId = sessionId, .runtimeIncarnationId = session->incarnationId, .alert = true });
    m_driver.post({
        .key = key,
        .title = notificationPlainText(session->name, 160),
        .body = notificationPlainText(text, 240),
    });
}

void TerminalNotifications::track(const QString& key, const Context& context)
{
    constexpr qsizetype maximumTracked = 256;
    while (!m_contexts.contains(key) && m_contexts.size() >= maximumTracked && !m_order.isEmpty()) {
        const auto oldest = m_order.head();
        remove(oldest, true);
    }
    if (!m_contexts.contains(key))
        m_order.enqueue(key);
    m_contexts.insert(key, context);
}

void TerminalNotifications::apply(const QJsonObject& event)
{
    if (event.value(QStringLiteral("type")) != QStringLiteral("term.notification")) return;
    const Context context {
        .sessionId = event.value(QStringLiteral("sessionId")).toString(),
        .runtimeIncarnationId = event.value(QStringLiteral("runtimeIncarnationId")).toString(),
    };
    if (!current(context)) {
        return;
    }

    const auto key = QStringLiteral("terminal:")
        + QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    auto title = notificationPlainText(event.value(QStringLiteral("title")).toString(), 160);
    auto body = notificationPlainText(event.value(QStringLiteral("body")).toString(), 240);
    if (title.isEmpty()) {
        title = tr("Terminal");
    }
    if (body.isEmpty()) {
        body = notificationPlainText(context.sessionId, 160);
    }
    track(key, context);
    m_driver.post({
        .key = key,
        .title = std::move(title),
        .body = std::move(body),
    });
}

void TerminalNotifications::handleAction(
    const QString& key,
    const QString& action,
    const QString& activationToken)
{
    if (!owns(key) || action != QStringLiteral("default")) {
        return;
    }
    const auto found = m_contexts.constFind(key);
    if (found == m_contexts.cend()) {
        m_driver.withdraw(key);
        return;
    }
    const auto context = found.value();
    if (!current(context)) {
        remove(key, true);
        return;
    }
    remove(key, true);
    emit sessionRequested(context.sessionId, activationToken);
}

void TerminalNotifications::remove(
    const QString& key,
    const bool withdraw)
{
    if (m_contexts.remove(key) == 0) {
        return;
    }
    m_order.removeAll(key);
    if (withdraw) {
        m_driver.withdraw(key);
    }
}

void TerminalNotifications::prune()
{
    const auto keys = m_contexts.keys();
    for (const auto& key : keys) {
        const auto context = m_contexts.constFind(key);
        if (context != m_contexts.cend() && !current(context.value())) {
            remove(key, true);
        }
    }
}

bool TerminalNotifications::current(const Context& context) const
{
    const auto session = m_sessions.session(context.sessionId);
    if (!m_sessions.hasAuthoritativeSnapshot() || !session || session->incarnationId != context.runtimeIncarnationId)
        return false;
    return context.alert ? session->isOwner && m_sessions.attention().contains(context.sessionId)
                         : session->kind == QStringLiteral("local");
}

bool TerminalNotifications::owns(const QString& key)
{
    return key.startsWith(QStringLiteral("terminal:"));
}

}
