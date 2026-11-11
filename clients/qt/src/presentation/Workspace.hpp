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
#include <QPointer>
#include <QStringList>
#include <QUrl>

#include <optional>

namespace kodosi {
class CanvasModel;
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
    // Opens the canvas of the terminal in front at what its agent shows and changes, once for each.
    void attachCanvas(CanvasModel& canvas);
    // Puts files on the canvas of a terminal and opens the canvas at its files where the person
    // is: in the room that shows the terminal, or else on the stage.
    Q_INVOKABLE void addFiles(const QString& sessionId, const QList<QUrl>& files);
    // The terminal that the person looks at while its view shows: the one in single view, the
    // selected one on the stage or the only one there, or the one that a room shows.
    [[nodiscard]] QString frontSessionId() const;

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
        bool quiet = false;
    };

    CommandDispatcher& m_commands;
    SessionCatalogModel& m_sessions;
    DesktopStateModel& m_desktop;
    DesktopSettings& m_settings;
    QPointer<CanvasModel> m_canvas;
    QHash<QString, Pending> m_pending;
    std::optional<std::pair<QStringList, bool>> m_reportedVisible;
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
        QString sessionId = {}, QString missionId = {}, bool quiet = false);
    bool hasPending(PendingDomain domain) const;
    void notifyBusy();
    void reportVisible();
    void openCanvas(const QString& sessionId, const QString& section);
    [[nodiscard]] bool canvasOpen(const QString& sessionId) const;
    void followCanvas();
    void askRestore();
    void receiveResult(const QString& type, const QJsonObject& event);
    void clearAccountData(bool clearLinks, const QString& userId);
    static bool validName(const QString& name);
};
}
