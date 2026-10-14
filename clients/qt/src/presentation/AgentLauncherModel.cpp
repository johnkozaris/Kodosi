#include "presentation/AgentLauncherModel.hpp"

#include <algorithm>
#include <utility>

#include <QDateTime>
#include <QJsonArray>
#include <QJsonDocument>
#include <QTimer>
#include <QUuid>

namespace kodosi {
namespace {
constexpr auto ChoiceKey = "launcher/choice";
constexpr qint64 RefreshMilliseconds = 8000;
constexpr int ReplyMilliseconds = 30000;
}

AgentLauncherModel::AgentLauncherModel(
    CommandDispatcher& commands, std::unique_ptr<QSettings> settings, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
    , m_settings(std::move(settings))
{
    m_choice = m_settings->value(QLatin1StringView(ChoiceKey)).toMap();
}

bool AgentLauncherModel::addingAccount() const
{
    return std::ranges::any_of(m_pending, [](const Pending& pending) { return pending.request == Request::Account; });
}

void AgentLauncherModel::refresh(bool force)
{
    const auto now = QDateTime::currentMSecsSinceEpoch();
    const bool asking = std::ranges::any_of(m_pending, [](const Pending& pending) { return pending.request == Request::Agents; });
    // The answer under way can be older than the change that forces a refresh.
    if (asking)
        m_refreshesAgain = m_refreshesAgain || force;
    if (asking || (!force && now - m_refreshedAt < RefreshMilliseconds))
        return;
    send(Request::Agents, {}, { { QStringLiteral("type"), QStringLiteral("provider.agents") } });
}

void AgentLauncherModel::loadModels(const QString& agent)
{
    if (agent.isEmpty() || m_models.contains(agent) || m_loading.contains(agent))
        return;
    const auto installed = std::ranges::any_of(m_agents, [&](const QVariant& value) {
        const auto info = value.toMap();
        return info.value(QStringLiteral("agent")).toString() == agent && info.value(QStringLiteral("installed")).toBool();
    });
    if (!installed)
        return;
    if (send(Request::Models, agent,
            { { QStringLiteral("type"), QStringLiteral("provider.agentModels") }, { QStringLiteral("agent"), agent } })) {
        m_loading.append(agent);
        emit modelsChanged();
    }
}

void AgentLauncherModel::addAccount(const QString& agent, const QString& name)
{
    if (addingAccount() || name.trimmed().isEmpty())
        return;
    m_accountError.clear();
    send(Request::Account, agent,
        { { QStringLiteral("type"), QStringLiteral("provider.addAccount") }, { QStringLiteral("agent"), agent },
            { QStringLiteral("name"), name.trimmed() } });
    emit accountErrorChanged();
}

void AgentLauncherModel::chooseAgent(const QString& agent)
{
    if (agent.isEmpty())
        m_choice.remove(QStringLiteral("agent"));
    else
        m_choice.insert(QStringLiteral("agent"), agent);
    persist();
}

void AgentLauncherModel::choose(const QString& kind, const QString& agent, const QString& value)
{
    if (kind != QStringLiteral("accounts") && kind != QStringLiteral("models") && kind != QStringLiteral("efforts"))
        return;
    auto values = m_choice.value(kind).toMap();
    if (value.isEmpty())
        values.remove(agent);
    else
        values.insert(agent, value);
    m_choice.insert(kind, values);
    persist();
}

void AgentLauncherModel::apply(const QJsonObject& event)
{
    const auto requestId = event.value(QStringLiteral("requestId")).toString();
    const auto found = m_pending.constFind(requestId);
    if (found == m_pending.constEnd())
        return;
    const auto pending = *found;
    m_pending.remove(requestId);
    const bool failed = event.value(QStringLiteral("type")).toString() == QStringLiteral("provider.error");
    const auto result = event.value(QStringLiteral("result")).toObject();
    switch (pending.request) {
    case Request::Agents:
        if (!failed) {
            m_refreshedAt = QDateTime::currentMSecsSinceEpoch();
            const bool known = !m_agents.isEmpty();
            QStringList before;
            for (const auto& value : std::as_const(m_agents)) {
                const auto info = value.toMap();
                if (info.value(QStringLiteral("installed")).toBool())
                    before.append(info.value(QStringLiteral("agent")).toString());
            }
            m_agents = result.value(QStringLiteral("agents")).toArray().toVariantList();
            QStringList installed;
            for (const auto& value : m_agents) {
                const auto info = value.toMap();
                if (info.value(QStringLiteral("installed")).toBool())
                    installed.append(info.value(QStringLiteral("agent")).toString());
            }
            if (known) {
                QStringList arrived;
                for (const auto& agent : std::as_const(installed)) {
                    if (!before.contains(agent) && !m_fresh.contains(agent))
                        arrived.append(agent);
                }
                announce(arrived);
            }
            for (const auto& agent : m_models.keys()) {
                if (!installed.contains(agent))
                    m_models.remove(agent);
            }
            emit agentsChanged();
            emit modelsChanged();
        }
        if (std::exchange(m_refreshesAgain, false))
            refresh(true);
        break;
    case Request::Models:
        m_loading.removeAll(pending.agent);
        if (!failed)
            m_models.insert(pending.agent, result.toVariantMap());
        emit modelsChanged();
        break;
    case Request::Account:
        if (failed) {
            m_accountError = event.value(QStringLiteral("message")).toString();
        } else {
            const auto command = result.value(QStringLiteral("command")).toString();
            for (auto& value : m_agents) {
                auto info = value.toMap();
                if (info.value(QStringLiteral("agent")).toString() != pending.agent)
                    continue;
                info.insert(QStringLiteral("accounts"),
                    info.value(QStringLiteral("accounts")).toList() << result.toVariantMap());
                value = info;
                emit agentsChanged();
            }
            choose(QStringLiteral("accounts"), pending.agent, command);
            emit accountAdded(pending.agent, command);
            refresh(true);
        }
        emit accountErrorChanged();
        break;
    }
}

void AgentLauncherModel::setGrown(bool grown)
{
    if (m_grown == grown)
        return;
    m_grown = grown;
    emit grownChanged();
}

void AgentLauncherModel::announce(const QStringList& agents)
{
    if (agents.isEmpty())
        return;
    m_fresh.append(agents);
    emit freshChanged();
    QTimer::singleShot(4000, this, [this, agents] {
        for (const auto& agent : agents)
            m_fresh.removeAll(agent);
        emit freshChanged();
    });
}

void AgentLauncherModel::reset()
{
    const bool adding = addingAccount();
    m_pending.clear();
    m_loading.clear();
    m_refreshesAgain = false;
    emit modelsChanged();
    if (adding)
        emit accountErrorChanged();
}

bool AgentLauncherModel::send(Request request, const QString& agent, QJsonObject values)
{
    const auto requestId = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    values.insert(QStringLiteral("requestId"), requestId);
    if (!m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact))) {
        if (request == Request::Account) {
            m_accountError = tr("Kodosi could not ask for a new account.");
            emit accountErrorChanged();
        }
        return false;
    }
    m_pending.insert(requestId, { request, agent });
    QTimer::singleShot(ReplyMilliseconds, this, [this, requestId] {
        const auto found = m_pending.constFind(requestId);
        if (found == m_pending.constEnd())
            return;
        apply(QJsonObject { { QStringLiteral("type"), QStringLiteral("provider.error") },
            { QStringLiteral("requestId"), requestId },
            { QStringLiteral("message"), tr("The agent did not answer in time.") } });
    });
    return true;
}

void AgentLauncherModel::persist()
{
    m_settings->setValue(QLatin1StringView(ChoiceKey), m_choice);
    emit choiceChanged();
}
}
