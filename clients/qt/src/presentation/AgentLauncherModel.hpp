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

public:
    AgentLauncherModel(CommandDispatcher& commands, std::unique_ptr<QSettings> settings,
        QObject* parent = nullptr);

    QVariantList agents() const { return m_agents; }
    QVariantMap models() const { return m_models; }
    QStringList loadingModels() const { return m_loading; }
    QVariantMap choice() const { return m_choice; }
    QString accountError() const { return m_accountError; }
    bool addingAccount() const;

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

    bool send(Request request, const QString& agent, QJsonObject values);
    void persist();
};
}
