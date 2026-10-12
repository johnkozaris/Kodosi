#pragma once
#include <QObject>
#include <QPointer>
#include <QSettings>
#include <QHash>
#include <QSize>
#include <QStringList>
#include <memory>
#include <optional>

class QWindow;
namespace kodosi {
class SessionCatalogModel;
class DesktopStateModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(ActiveView activeView READ activeView WRITE setActiveView NOTIFY activeViewChanged)
    Q_PROPERTY(bool sidebarOpen READ sidebarOpen WRITE setSidebarOpen NOTIFY sidebarOpenChanged)
    Q_PROPERTY(QString selectedSessionId READ selectedSessionId NOTIFY selectedSessionIdChanged)
    Q_PROPERTY(QStringList stagedSessionIds READ stagedSessionIds NOTIFY stagedSessionIdsChanged)
    Q_PROPERTY(StageLayoutMode stageLayoutMode READ stageLayoutMode NOTIFY stageLayoutModeChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(bool modalOpen READ modalOpen NOTIFY modalOpenChanged)
public:
    enum ActiveView { Sessions, Missions, People, Settings };
    Q_ENUM(ActiveView)
    enum StageLayoutMode { Grid, Focus };
    Q_ENUM(StageLayoutMode)
    explicit DesktopStateModel(QObject* parent = nullptr);
    DesktopStateModel(
        std::unique_ptr<QSettings> settings, bool persistenceEnabled, QObject* parent = nullptr);
    ~DesktopStateModel() override;
    ActiveView activeView() const { return m_activeView; }
    bool sidebarOpen() const { return m_sidebarOpen; }
    QString selectedSessionId() const { return m_selected; }
    QStringList stagedSessionIds() const { return m_staged; }
    StageLayoutMode stageLayoutMode() const { return m_mode; }
    QString lastError() const { return m_error; }
    void setActiveView(ActiveView value);
    void setSidebarOpen(bool value);
    void attachSessionCatalog(SessionCatalogModel* sessions);
    void attachWindow(QWindow* window, std::optional<QSize> sizeOverride = std::nullopt);
    Q_INVOKABLE bool selectSession(const QString& id);
    Q_INVOKABLE bool stageSession(const QString& id, bool revealStage = true);
    Q_INVOKABLE bool unstageSession(const QString& id);
    Q_INVOKABLE void enterFocusMode(const QString& id = {});
    Q_INVOKABLE void exitFocusMode();
    Q_INVOKABLE void toggleFocusForSelectedSession();
    bool modalOpen() const { return !m_modals.isEmpty(); }
    Q_INVOKABLE void setModalVisible(QObject* modal, bool visible);
    Q_INVOKABLE void clearError();
    void clearSessions();
signals:
    void activeViewChanged();
    void sidebarOpenChanged();
    void selectedSessionIdChanged();
    void stagedSessionIdsChanged();
    void stageLayoutModeChanged();
    void lastErrorChanged();
    void modalOpenChanged();

protected:
    bool eventFilter(QObject*, QEvent*) override;

private:
    std::unique_ptr<QSettings> m_settings;
    QPointer<SessionCatalogModel> m_sessions;
    QPointer<QWindow> m_window;
    QHash<QObject*, QMetaObject::Connection> m_modals;
    QString m_selected;
    QStringList m_staged;
    QString m_error;
    ActiveView m_activeView = Sessions;
    bool m_sidebarOpen = true;
    bool m_persistenceEnabled = true;
    StageLayoutMode m_mode = Grid;
    void reconcile();
    void saveWindow();
};
}
