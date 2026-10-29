#pragma once
#include <QObject>
#include <QPointer>
#include <QSettings>
#include <QHash>
#include <QSet>
#include <QSize>
#include <QStringList>
#include <QVariantMap>
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
    Q_PROPERTY(QStringList pinnedSessionIds READ pinnedSessionIds NOTIFY pinnedSessionIdsChanged)
    Q_PROPERTY(QStringList openCanvases READ openCanvases NOTIFY openCanvasesChanged)
    Q_PROPERTY(StageLayoutMode stageLayoutMode READ stageLayoutMode NOTIFY stageLayoutModeChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(bool modalOpen READ modalOpen NOTIFY modalOpenChanged)
    Q_PROPERTY(QString launcherPrompt READ launcherPrompt WRITE setLauncherPrompt NOTIFY launcherDraftChanged)
    Q_PROPERTY(QString launcherFolder READ launcherFolder WRITE setLauncherFolder NOTIFY launcherDraftChanged)
    Q_PROPERTY(QVariantMap launcherRemoteFolders READ launcherRemoteFolders NOTIFY launcherDraftChanged)
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
    QStringList pinnedSessionIds() const { return m_pins; }
    Q_INVOKABLE void togglePin(const QString& id);
    Q_INVOKABLE bool moveStaged(const QString& id, const QString& target);
    QStringList openCanvases() const { return m_canvases; }
    Q_INVOKABLE void toggleCanvas(const QString& id);
    Q_INVOKABLE void closeCanvas(const QString& id);
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
    void switchAccount(const QString& userId);
    void noteClosing(const QString& id);
    QString launcherPrompt() const { return m_launcherPrompt; }
    QString launcherFolder() const { return m_launcherFolder; }
    QVariantMap launcherRemoteFolders() const { return m_launcherRemoteFolders; }
    void setLauncherPrompt(const QString& value);
    void setLauncherFolder(const QString& value);
    Q_INVOKABLE void setLauncherRemoteFolder(const QString& computer, const QString& path);
    Q_INVOKABLE void clearLauncherDraft();
signals:
    void activeViewChanged();
    void sidebarOpenChanged();
    void dosiOpenChanged();
    void dosiOnChanged();
    void sidebarWidthChanged();
    void selectedSessionIdChanged();
    void stagedSessionIdsChanged();
    void pinnedSessionIdsChanged();
    void openCanvasesChanged();
    void stageLayoutModeChanged();
    void lastErrorChanged();
    void modalOpenChanged();
    void launcherDraftChanged();
    void restored(const QStringList& sessionIds);

protected:
    bool eventFilter(QObject*, QEvent*) override;

private:
    struct Computer {
        QString owner;
        QString host;
    };
    struct Departure {
        QString id;
        Computer computer;
        qsizetype place = 0;
        bool selected = false;
        bool focused = false;
    };
    std::unique_ptr<QSettings> m_settings;
    QPointer<SessionCatalogModel> m_sessions;
    QPointer<QWindow> m_window;
    QHash<QObject*, QMetaObject::Connection> m_modals;
    QString m_selected;
    QStringList m_staged;
    QStringList m_pins;
    QStringList m_canvases;
    QHash<QString, Computer> m_pinComputers;
    QString m_stageKey;
    QStringList m_restoring;
    QStringList m_savedStage;
    QHash<QString, Computer> m_computers;
    QList<Departure> m_departures;
    QSet<QString> m_closing;
    QString m_error;
    QString m_launcherPrompt;
    QString m_launcherFolder;
    QVariantMap m_launcherRemoteFolders;
    ActiveView m_activeView = Sessions;
    bool m_sidebarOpen = true;
    bool m_dosiOpen = false;
    bool m_dosiOn = true;
    qreal m_listWidth = 252;
    qreal m_dosiWidth = 420;
    StageLayoutMode m_mode = Grid;
    void exitFocusMode();
    void reconcile();
    void remember(const QString& id);
    void depart(const QString& id, qsizetype place);
    QStringList follow();
    void saveStage();
    void followPins();
    void savePins();
    void saveWindow();
};
}
