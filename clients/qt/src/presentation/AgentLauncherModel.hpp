#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QSettings>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

#include <memory>

namespace kodosi {
/// The agents of this computer, their models, and the choices of the launcher.
class AgentLauncherModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList agents READ agents NOTIFY agentsChanged)
    Q_PROPERTY(QVariantMap models READ models NOTIFY modelsChanged)
    Q_PROPERTY(QStringList loadingModels READ loadingModels NOTIFY modelsChanged)
    Q_PROPERTY(QVariantMap choice READ choice NOTIFY choiceChanged)
    Q_PROPERTY(QString accountError READ accountError NOTIFY accountErrorChanged)
    Q_PROPERTY(bool addingAccount READ addingAccount NOTIFY accountErrorChanged)
    Q_PROPERTY(QStringList fresh READ fresh NOTIFY freshChanged)
    Q_PROPERTY(bool grown READ grown WRITE setGrown NOTIFY grownChanged)

public:
    AgentLauncherModel(CommandDispatcher& commands, std::unique_ptr<QSettings> settings,
        QObject* parent = nullptr);

    QVariantList agents() const { return m_agents; }
    QVariantMap models() const { return m_models; }
    QStringList loadingModels() const { return m_loading; }
    QVariantMap choice() const { return m_choice; }
    QString accountError() const { return m_accountError; }
    bool addingAccount() const;
    /// Agents that this computer just got, while the launcher says so.
    QStringList fresh() const { return m_fresh; }
    /// Whether the home's terminal grew from its cursor in this run of the app.
    bool grown() const { return m_grown; }
    void setGrown(bool grown);

    Q_INVOKABLE void refresh(bool force = false);
    Q_INVOKABLE void loadModels(const QString& agent);
    Q_INVOKABLE void addAccount(const QString& agent, const QString& name);
    Q_INVOKABLE void chooseAgent(const QString& agent);
    Q_INVOKABLE void choose(const QString& kind, const QString& agent, const QString& value);

    void apply(const QJsonObject& event);
    void reset();

signals:
    void agentsChanged();
    void modelsChanged();
    void choiceChanged();
    void accountErrorChanged();
    void accountAdded(QString agent, QString command);
    void freshChanged();
    void grownChanged();

private:
    enum class Request { Agents, Models, Account };
    struct Pending {
        Request request;
        QString agent;
    };

    CommandDispatcher& m_commands;
    std::unique_ptr<QSettings> m_settings;
    QHash<QString, Pending> m_pending;
    QVariantList m_agents;
    QVariantMap m_models;
    QStringList m_loading;
    QVariantMap m_choice;
    QString m_accountError;
    qint64 m_refreshedAt = 0;
    bool m_refreshesAgain = false;
    QStringList m_fresh;
    bool m_grown = false;

    bool send(Request request, const QString& agent, QJsonObject values);
    void announce(const QStringList& agents);
    void persist();
};
}
