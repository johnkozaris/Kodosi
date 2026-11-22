#include "platform/TerminalNotifications.hpp"

#include "platform/NotificationText.hpp"

#include <optional>
#include <utility>

namespace kodosi {
namespace {

const auto keyPrefix = QStringLiteral("terminal:");

QString keyOf(const QString& sessionId)
{
    return keyPrefix + sessionId;
}

std::optional<NotificationCue> cueOf(const QJsonValue& value)
{
    const auto cue = value.toString();
    if (cue == QStringLiteral("sound"))
        return NotificationCue::Sound;
    if (cue == QStringLiteral("banner"))
        return NotificationCue::Banner;
    if (cue == QStringLiteral("list"))
        return NotificationCue::List;
    return std::nullopt;
}

}

TerminalNotifications::TerminalNotifications(
    SessionCatalogModel& sessions,
    DesktopNotificationDriver& driver,
    QObject* parent)
    : QObject(parent)
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
    connect(&m_driver, &DesktopNotificationDriver::notificationClosed, this,
        [this](const QString& key) { m_contexts.remove(key); });
    connect(&m_sessions, &SessionCatalogModel::authoritativeSnapshotApplied, this, [this] { prune(); });
    connect(&m_sessions, &SessionCatalogModel::attentionChanged, this, [this] { prune(); });
}

void TerminalNotifications::apply(const QJsonObject& event)
{
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type != QStringLiteral("term.attention") && type != QStringLiteral("term.notification"))
        return;
    const auto sessionId = event.value(QStringLiteral("sessionId")).toString();
    const auto cue = cueOf(event.value(QStringLiteral("cue")));
    if (!cue)
        return;
    if (type == QStringLiteral("term.attention")) {
        const auto session = m_sessions.session(sessionId);
        if (!session)
            return;
        post({ .sessionId = sessionId, .runtimeIncarnationId = session->incarnationId, .origin = Origin::Mark },
            notificationPlainText(session->name, 160),
            notificationPlainText(m_sessions.attentionText(sessionId), 240), *cue);
        return;
    }
    auto title = notificationPlainText(event.value(QStringLiteral("title")).toString(), 160);
    auto body = notificationPlainText(event.value(QStringLiteral("body")).toString(), 240);
    if (title.isEmpty())
        title = tr("Terminal");
    if (body.isEmpty())
        body = notificationPlainText(sessionId, 160);
    post({ .sessionId = sessionId,
             .runtimeIncarnationId = event.value(QStringLiteral("runtimeIncarnationId")).toString(),
             .origin = Origin::Program },
        std::move(title), std::move(body), *cue);
}

void TerminalNotifications::setNotifies(const bool notifies)
{
    m_notifies = notifies;
    if (notifies)
        return;
    for (const auto& key : m_contexts.keys())
        remove(key);
}

void TerminalNotifications::post(const Context& context, QString title, QString body, const NotificationCue cue)
{
    const auto session = m_sessions.session(context.sessionId);
    if (!m_notifies || !session || !current(context))
        return;
    const auto key = keyOf(context.sessionId);
    m_contexts.insert(key, context);
    m_driver.post({
        .key = key,
        .title = std::move(title),
        .body = std::move(body),
        .sessionId = context.sessionId,
        .computer = session->hostDeviceId,
        .cue = cue,
    });
}

void TerminalNotifications::handleAction(
    const QString& key,
    const QString& action,
    const QString& activationToken)
{
    if (!key.startsWith(keyPrefix) || action != QStringLiteral("default"))
        return;
    const auto sessionId = DeepLinkRouter::sessionId(QStringView(key).sliced(keyPrefix.size()));
    if (!sessionId)
        return;
    remove(key);
    emit sessionRequested(*sessionId, activationToken);
}

void TerminalNotifications::remove(const QString& key)
{
    m_contexts.remove(key);
    m_driver.withdraw(key);
}

void TerminalNotifications::prune()
{
    const auto contexts = m_contexts;
    for (auto context = contexts.cbegin(); context != contexts.cend(); ++context) {
        if (!current(context.value()))
            remove(context.key());
    }
}

bool TerminalNotifications::current(const Context& context) const
{
    const auto session = m_sessions.session(context.sessionId);
    if (!m_sessions.hasAuthoritativeSnapshot() || !session)
        return false;
    if (!m_sessions.attention().contains(context.sessionId))
        return false;
    if (context.origin == Origin::Mark)
        return m_sessions.waiting(context.sessionId);
    return session->kind == QStringLiteral("local") && session->incarnationId == context.runtimeIncarnationId;
}

}
