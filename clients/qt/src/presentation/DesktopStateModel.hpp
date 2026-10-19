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
    Q_PROPERTY(bool dosiOpen READ dosiOpen WRITE setDosiOpen NOTIFY dosiOpenChanged)
    Q_PROPERTY(bool dosiOn READ dosiOn WRITE setDosiOn NOTIFY dosiOnChanged)
    Q_PROPERTY(qreal sidebarWidth READ sidebarWidth NOTIFY sidebarWidthChanged)
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
    explicit DesktopStateModel(std::unique_ptr<QSettings> settings, QObject* parent = nullptr);
    ~DesktopStateModel() override;
    ActiveView activeView() const { return m_activeView; }
    bool sidebarOpen() const { return m_sidebarOpen; }
    bool dosiOpen() const { return m_dosiOpen; }
    bool dosiOn() const { return m_dosiOn; }
    qreal sidebarWidth() const;
    QString selectedSessionId() const { return m_selected; }
    QStringList stagedSessionIds() const { return m_staged; }
    StageLayoutMode stageLayoutMode() const { return m_mode; }
    QString lastError() const { return m_error; }
    void setActiveView(ActiveView value);
    void setSidebarOpen(bool value);
    void setDosiOpen(bool value);
    void setDosiOn(bool value);
    Q_INVOKABLE void openDosi();
    Q_INVOKABLE void toggleDosi();
    Q_INVOKABLE void resizeSidebar(qreal width);
    void attachSessionCatalog(SessionCatalogModel* sessions);
    void attachWindow(QWindow* window, std::optional<QSize> sizeOverride = std::nullopt);
    Q_INVOKABLE bool selectSession(const QString& id);
    bool stageSession(const QString& id, bool revealStage = true);
    bool unstageSession(const QString& id);
    Q_INVOKABLE void toggleFocusForSelectedSession();
    bool modalOpen() const { return !m_modals.isEmpty(); }
    Q_INVOKABLE void setModalVisible(QObject* modal, bool visible);
    Q_INVOKABLE void clearError();
    void clearSessions();
signals:
    void activeViewChanged();
    void sidebarOpenChanged();
    void dosiOpenChanged();
    void dosiOnChanged();
    void sidebarWidthChanged();
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
    bool m_dosiOpen = false;
    bool m_dosiOn = true;
    qreal m_listWidth = 252;
    qreal m_dosiWidth = 420;
    StageLayoutMode m_mode = Grid;
    void exitFocusMode();
    void reconcile();
    void saveWindow();
};
}
