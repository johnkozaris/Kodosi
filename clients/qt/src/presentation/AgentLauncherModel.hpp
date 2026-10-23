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
    Q_PROPERTY(kodosi::AgentLauncherModel* remote READ remote CONSTANT)
    Q_PROPERTY(QString computer READ computer WRITE setComputer NOTIFY computerChanged)
    Q_PROPERTY(QVariantList agents READ agents NOTIFY agentsChanged)
    Q_PROPERTY(QVariantMap models READ models NOTIFY modelsChanged)
    Q_PROPERTY(QStringList loadingModels READ loadingModels NOTIFY modelsChanged)
    Q_PROPERTY(QVariantMap choice READ choice NOTIFY choiceChanged)
    Q_PROPERTY(QVariantMap plans READ plans NOTIFY plansChanged)
    Q_PROPERTY(QString accountError READ accountError NOTIFY accountErrorChanged)
    Q_PROPERTY(bool addingAccount READ addingAccount NOTIFY accountErrorChanged)
    Q_PROPERTY(QVariantMap usage READ usage NOTIFY usageChanged)
    Q_PROPERTY(bool usageAvailable READ usageAvailable NOTIFY usageChanged)
    Q_PROPERTY(QVariantList usageSections READ usageSections NOTIFY usageChanged)
    Q_PROPERTY(QVariant usageNearest READ usageNearest NOTIFY usageChanged)
    Q_PROPERTY(QVariantMap places READ places NOTIFY foldersChanged)
    Q_PROPERTY(QVariantMap listing READ listing NOTIFY foldersChanged)
    Q_PROPERTY(QString folderError READ folderError NOTIFY foldersChanged)

public:
    explicit AgentLauncherModel(CommandDispatcher& commands, QObject* parent = nullptr, bool remote = false);

    AgentLauncherModel* remote() const { return m_remote; }
    QString computer() const { return m_computer; }
    void setComputer(const QString& computer);
    QVariantList agents() const { return m_agents; }
    QVariantMap models() const { return m_models; }
    QStringList loadingModels() const { return m_loading; }
    QVariantMap choice() const { return m_choice; }
    QVariantMap plans() const;
    QString accountError() const { return m_accountError; }
    bool addingAccount() const;
    QVariantMap usage() const { return m_usage; }
    bool usageRead() const { return m_usageRead; }
    bool usageAvailable() const;
    QVariantList usageSections() const;
    QVariant usageNearest() const;
    QVariantMap places() const { return m_places; }
    QVariantMap listing() const { return m_listing; }
    QString folderError() const { return m_folderError; }

    Q_INVOKABLE void refresh(bool force = false);
    Q_INVOKABLE void loadModels(const QString& agent);
    Q_INVOKABLE void refreshUsage();
    Q_INVOKABLE QString accountTitle(const QString& name, const QString& program) const;
    Q_INVOKABLE void addAccount(const QString& agent, const QString& name);
    Q_INVOKABLE void chooseAgent(const QString& agent);
    Q_INVOKABLE void choose(const QString& kind, const QString& agent, const QString& value);
    Q_INVOKABLE void listFolders(const QString& path = {}, bool browse = false);
    Q_INVOKABLE void closeFolders();

    void apply(const QJsonObject& event);
    void reset();

signals:
    void computerChanged();
    void agentsChanged();
    void modelsChanged();
    void choiceChanged();
    void plansChanged();
    void accountErrorChanged();
    void foldersChanged();
    void usageChanged();
    void accountAdded(QString agent, QString name);

private:
    enum class Request { Agents, Models, Account, Choose, Folders, Usage };
    struct Pending {
        Request request;
        QString agent;
        int asOf;
        bool browse = false;
    };

    CommandDispatcher& m_commands;
    AgentLauncherModel* m_remote = nullptr;
    QString m_computer;
    QHash<QString, Pending> m_pending;
    QVariantList m_agents;
    QVariantMap m_models;
    QStringList m_loading;
    QVariantMap m_choice;
    QString m_accountError;
    QVariantMap m_usage;
    bool m_usageRead = false;
    QVariantMap m_places;
    QVariantMap m_listing;
    QString m_folderError;
    QString m_browsing;
    qint64 m_refreshedAt = 0;
    int m_changes = 0;
    bool m_refreshesAgain = false;

    QVariantMap plan(const QVariantMap& info) const;
    QString send(Request request, const QString& agent, QJsonObject values, bool browse = false);
    void change(const QJsonObject& change);
    void adopt(const QJsonObject& choice, int asOf);
};
}
