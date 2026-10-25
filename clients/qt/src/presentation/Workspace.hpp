#pragma once

#include "app/DeepLinkRouter.hpp"
#include "presentation/AccountModel.hpp"
#include "presentation/DevicesModel.hpp"
#include "presentation/DosiModel.hpp"
#include "presentation/MissionsModel.hpp"
#include "presentation/PeopleModel.hpp"
#include "presentation/SessionActionsModel.hpp"

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QStringList>

#include <optional>

namespace kodosi {
class CommandDispatcher;
class DesktopSettings;
class DesktopStateModel;
class SessionCatalogModel;

class Workspace final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)

public:
    Workspace(CommandDispatcher& commands, SessionCatalogModel& sessions,
        DesktopStateModel& desktop, DesktopSettings& settings, QObject* parent = nullptr);

    QString error() const { return m_error; }
    AccountModel& account() { return m_account; }
    PeopleModel& people() { return m_people; }
    DevicesModel& devices() { return m_devices; }
    MissionsModel& missions() { return m_missions; }
    DosiModel& dosi() { return m_dosi; }
    SessionActionsModel& sessionActions() { return m_sessionActions; }

    Q_INVOKABLE void clearError();
    void apply(const QJsonObject& event, std::uint64_t accountEpoch);
    void reset();
    void route(const DeepLinkDestination& destination);
    void setError(QString error);

signals:
    void errorChanged();
    void hostReady();
    void showRequested(const QString& sessionId);

private:
    friend class AccountModel;
    friend class DevicesModel;
    friend class DosiModel;
    friend class MissionsModel;
    friend class PeopleModel;
    friend class SessionActionsModel;

    enum class PendingDomain { Session, Mission };
    struct Pending {
        PendingDomain domain;
        QString operation;
        QString sessionId;
        QString missionId;
        QVariantMap draft;
    };

    CommandDispatcher& m_commands;
    SessionCatalogModel& m_sessions;
    DesktopStateModel& m_desktop;
    QHash<QString, Pending> m_pending;
    std::optional<QStringList> m_reportedVisible;
    bool m_hostReady = false;
    bool m_restoreAsked = false;
    QString m_error;
    AccountModel m_account;
    PeopleModel m_people;
    DevicesModel m_devices;
    MissionsModel m_missions;
    SessionActionsModel m_sessionActions;
    DosiModel m_dosi;

    QString sendRequest(QString type, QJsonObject values = {});
    bool sendUntracked(QString type, QJsonObject values = {});
    bool sendTracked(PendingDomain domain, QString type, QJsonObject values = {},
        QString sessionId = {}, QString missionId = {});
    bool hasPending(PendingDomain domain) const;
    void notifyBusy();
    void reportVisible();
    void askRestore();
    void receiveResult(const QString& type, const QJsonObject& event);
    void clearAccountData(bool clearLinks, const QString& userId);
    static bool validName(const QString& name);
};
}
