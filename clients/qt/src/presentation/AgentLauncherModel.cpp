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
constexpr qint64 RefreshMilliseconds = 8000;
constexpr int ReplyMilliseconds = 30000;

QString field(const QVariant& entry, const char* key)
{
    return entry.toMap().value(QLatin1StringView(key)).toString();
}

QJsonValue nullable(const QString& value)
{
    return value.isEmpty() ? QJsonValue() : QJsonValue(value);
}
}

AgentLauncherModel::AgentLauncherModel(CommandDispatcher& commands, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
{
    for (const auto signal : { &AgentLauncherModel::agentsChanged, &AgentLauncherModel::modelsChanged,
             &AgentLauncherModel::choiceChanged })
        connect(this, signal, this, &AgentLauncherModel::plansChanged);
}

QVariantMap AgentLauncherModel::plans() const
{
    QVariantMap plans;
    for (const auto& value : m_agents) {
        const auto info = value.toMap();
        plans.insert(info.value(QStringLiteral("agent")).toString(), plan(info));
    }
    return plans;
}

QVariantMap AgentLauncherModel::plan(const QVariantMap& info) const
{
    const auto agent = info.value(QStringLiteral("agent")).toString();
    const auto id = [](const QVariant& entry) { return field(entry, "id"); };
    const auto picks = m_choice.value(QStringLiteral("picks")).toList();
    const auto picked = std::ranges::find(picks, agent, [](const QVariant& entry) { return field(entry, "agent"); });
    const auto saved = picked == picks.end() ? QVariantMap() : picked->toMap();
    const auto accounts = info.value(QStringLiteral("accounts")).toList();
    const auto chosen = std::ranges::find(accounts, saved.value(QStringLiteral("account")).toString(),
        [](const QVariant& entry) { return field(entry, "name"); });
    const auto account = chosen == accounts.end() ? accounts.value(0) : *chosen;
    const bool listed = m_models.contains(agent);
    const auto offer = m_models.value(agent).toMap();
    auto modelId = saved.value(QStringLiteral("model")).toString();
    const auto models = offer.value(QStringLiteral("models")).toList();
    const auto model = std::ranges::find(models, modelId, id);
    if (listed && model == models.end())
        modelId.clear();
    const auto allowed = model == models.end() ? QStringList() : model->toMap().value(QStringLiteral("efforts")).toStringList();
    QVariantList efforts;
    for (const auto& entry : offer.value(QStringLiteral("efforts")).toList()) {
        if (allowed.isEmpty() || allowed.contains(id(entry)))
            efforts.append(entry);
    }
    auto effortId = saved.value(QStringLiteral("effort")).toString();
    if (listed && std::ranges::find(efforts, effortId, id) == efforts.end())
        effortId.clear();
    QVariant start;
    if (info.value(QStringLiteral("installed")).toBool() && account.isValid()) {
        start = QVariantMap { { QStringLiteral("agent"), agent }, { QStringLiteral("account"), field(account, "name") },
            { QStringLiteral("model"), modelId }, { QStringLiteral("effort"), effortId } };
    }
    return { { QStringLiteral("account"), account }, { QStringLiteral("modelId"), modelId },
        { QStringLiteral("model"), model == models.end() ? QVariant() : *model },
        { QStringLiteral("efforts"), efforts }, { QStringLiteral("effortId"), effortId },
        { QStringLiteral("start"), start } };
}

bool AgentLauncherModel::addingAccount() const
{
    return std::ranges::any_of(m_pending, [](const Pending& pending) { return pending.request == Request::Account; });
}

void AgentLauncherModel::refresh(bool force)
{
    const auto now = QDateTime::currentMSecsSinceEpoch();
    const bool asking = std::ranges::any_of(m_pending, [](const Pending& pending) { return pending.request == Request::Agents; });
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
    m_choice.insert(QStringLiteral("agent"), agent.isEmpty() ? QVariant() : QVariant(agent));
    emit choiceChanged();
    change({ { QStringLiteral("type"), QStringLiteral("tab") }, { QStringLiteral("agent"), nullable(agent) } });
}

void AgentLauncherModel::choose(const QString& kind, const QString& agent, const QString& value)
{
    const auto info = std::ranges::find(m_agents, agent, [](const QVariant& entry) { return field(entry, "agent"); });
    if (info == m_agents.end()
        || (kind != QStringLiteral("account") && kind != QStringLiteral("model") && kind != QStringLiteral("effort")))
        return;
    const auto shown = plan(info->toMap());
    QJsonObject pick { { QStringLiteral("agent"), agent },
        { QStringLiteral("account"), nullable(field(shown.value(QStringLiteral("account")), "name")) },
        { QStringLiteral("model"), nullable(shown.value(QStringLiteral("modelId")).toString()) },
        { QStringLiteral("effort"), nullable(shown.value(QStringLiteral("effortId")).toString()) } };
    pick.insert(kind, nullable(value));
    auto picks = m_choice.value(QStringLiteral("picks")).toList();
    const auto saved = std::ranges::find(picks, agent, [](const QVariant& entry) { return field(entry, "agent"); });
    if (saved == picks.end())
        picks.append(pick.toVariantMap());
    else
        *saved = pick.toVariantMap();
    m_choice.insert(QStringLiteral("picks"), picks);
    emit choiceChanged();
    pick.insert(QStringLiteral("type"), QStringLiteral("pick"));
    change(pick);
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
            adopt(result.value(QStringLiteral("choice")).toObject(), pending.asOf);
            auto agents = result.value(QStringLiteral("agents")).toArray().toVariantList();
            if (agents != m_agents) {
                m_agents = std::move(agents);
                QStringList installed;
                for (const auto& value : std::as_const(m_agents)) {
                    const auto info = value.toMap();
                    if (info.value(QStringLiteral("installed")).toBool())
                        installed.append(info.value(QStringLiteral("agent")).toString());
                }
                for (const auto& agent : m_models.keys()) {
                    if (!installed.contains(agent))
                        m_models.remove(agent);
                }
                emit agentsChanged();
                emit modelsChanged();
            }
        }
        if (std::exchange(m_refreshesAgain, false))
            refresh(true);
        break;
    case Request::Models:
        m_loading.removeAll(pending.agent);
        if (!failed) {
            m_models.insert(pending.agent,
                QVariantMap { { QStringLiteral("models"), result.value(QStringLiteral("models")).toArray().toVariantList() },
                    { QStringLiteral("efforts"), result.value(QStringLiteral("efforts")).toArray().toVariantList() } });
            adopt(result.value(QStringLiteral("choice")).toObject(), pending.asOf);
        }
        emit modelsChanged();
        break;
    case Request::Account:
        if (failed) {
            m_accountError = event.value(QStringLiteral("message")).toString();
        } else {
            for (auto& value : m_agents) {
                auto info = value.toMap();
                if (info.value(QStringLiteral("agent")).toString() != pending.agent)
                    continue;
                info.insert(QStringLiteral("accounts"),
                    info.value(QStringLiteral("accounts")).toList() << result.toVariantMap());
                value = info;
                emit agentsChanged();
            }
            const auto name = result.value(QStringLiteral("name")).toString();
            choose(QStringLiteral("account"), pending.agent, name);
            emit accountAdded(pending.agent, name);
            refresh(true);
        }
        emit accountErrorChanged();
        break;
    case Request::Choose:
        if (!failed)
            adopt(result, pending.asOf);
        break;
    }
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
    m_pending.insert(requestId, { request, agent, m_changes });
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

void AgentLauncherModel::change(const QJsonObject& change)
{
    ++m_changes;
    send(Request::Choose, {}, { { QStringLiteral("type"), QStringLiteral("provider.choose") }, { QStringLiteral("change"), change } });
}

void AgentLauncherModel::adopt(const QJsonObject& choice, int asOf)
{
    auto chosen = choice.toVariantMap();
    if (asOf != m_changes || chosen == m_choice)
        return;
    m_choice = std::move(chosen);
    emit choiceChanged();
}
}
