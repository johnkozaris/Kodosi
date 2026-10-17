#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

namespace kodosi {
class AgentLauncherModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList agents READ agents NOTIFY agentsChanged)
    Q_PROPERTY(QVariantMap models READ models NOTIFY modelsChanged)
    Q_PROPERTY(QStringList loadingModels READ loadingModels NOTIFY modelsChanged)
    Q_PROPERTY(QVariantMap choice READ choice NOTIFY choiceChanged)
    Q_PROPERTY(QVariantMap plans READ plans NOTIFY plansChanged)
    Q_PROPERTY(QString accountError READ accountError NOTIFY accountErrorChanged)
    Q_PROPERTY(bool addingAccount READ addingAccount NOTIFY accountErrorChanged)

public:
    explicit AgentLauncherModel(CommandDispatcher& commands, QObject* parent = nullptr);

    QVariantList agents() const { return m_agents; }
    QVariantMap models() const { return m_models; }
    QStringList loadingModels() const { return m_loading; }
    QVariantMap choice() const { return m_choice; }
    QVariantMap plans() const;
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
    void plansChanged();
    void accountErrorChanged();
    void accountAdded(QString agent, QString name);

private:
    enum class Request { Agents, Models, Account, Choose };
    struct Pending {
        Request request;
        QString agent;
        int asOf;
    };

    CommandDispatcher& m_commands;
    QHash<QString, Pending> m_pending;
    QVariantList m_agents;
    QVariantMap m_models;
    QStringList m_loading;
    QVariantMap m_choice;
    QString m_accountError;
    qint64 m_refreshedAt = 0;
    int m_changes = 0;
    bool m_refreshesAgain = false;

    QVariantMap plan(const QVariantMap& info) const;
    bool send(Request request, const QString& agent, QJsonObject values);
    void change(const QJsonObject& change);
    void adopt(const QJsonObject& choice, int asOf);
};
}
