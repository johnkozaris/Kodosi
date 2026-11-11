#pragma once
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QSettings>
#include <QHash>
#include <QSet>
#include <QSize>
#include <QStringList>
#include <QTimer>
#include <QVariantMap>
#include <memory>
#include <optional>

class QWindow;
namespace kodosi {
class CanvasModel;
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
    Q_PROPERTY(QString canvasSessionId READ canvasSessionId NOTIFY canvasChanged)
    Q_PROPERTY(bool canvasWide READ canvasWide WRITE setCanvasWide NOTIFY canvasChanged)
    Q_PROPERTY(qreal canvasWidth READ canvasWidth WRITE setCanvasWidth NOTIFY canvasChanged)
    Q_PROPERTY(StageLayoutMode stageLayoutMode READ stageLayoutMode NOTIFY stageLayoutModeChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(bool modalOpen READ modalOpen NOTIFY modalOpenChanged)
    // The terminals where the person dismissed the note that their agent, started by hand, does
    // not see Kodosi.
    Q_PROPERTY(QStringList notedOutside READ notedOutside NOTIFY notedOutsideChanged)
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
    QString canvasSessionId() const { return m_canvas; }
    bool canvasWide() const { return m_canvasWide; }
    qreal canvasWidth() const { return m_canvasWidth; }
    void setCanvasWide(bool value);
    void setCanvasWidth(qreal value);
    // Shows a terminal alone on the stage with its canvas beside it, at a part or else at
    // something, now or when its canvas has something to show on this device.
    Q_INVOKABLE void openCanvas(const QString& id, const QString& section = {});
    Q_INVOKABLE void hideCanvas();
    // The person closes the canvas, which a change does not open again until it opens.
    Q_INVOKABLE void closeCanvas();
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
    void attachCanvas(CanvasModel* canvas);
    void attachWindow(QWindow* window, std::optional<QSize> sizeOverride = std::nullopt);
    Q_INVOKABLE void bringToFront(const QString& activationToken = {});
    Q_INVOKABLE bool selectSession(const QString& id);
    bool stageSession(const QString& id, bool revealStage = true);
    bool unstageSession(const QString& id);
    QString makingRoom(const QString& id) const;
    Q_INVOKABLE void toggleFocusForSelectedSession();
    bool modalOpen() const { return !m_modals.isEmpty(); }
    bool watching() const;
    Q_INVOKABLE void setModalVisible(QObject* modal, bool visible);
    Q_INVOKABLE void clearError();
    QStringList notedOutside() const { return m_notedOutside; }
    Q_INVOKABLE void noteOutside(const QString& id);
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
    void watchingChanged();
    void activeViewChanged();
    void sidebarOpenChanged();
    void dosiOpenChanged();
    void dosiOnChanged();
    void sidebarWidthChanged();
    void selectedSessionIdChanged();
    void stagedSessionIdsChanged();
    void pinnedSessionIdsChanged();
    void canvasChanged();
    void stageLayoutModeChanged();
    void lastErrorChanged();
    void modalOpenChanged();
    void notedOutsideChanged();
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
    QPointer<CanvasModel> m_canvasModel;
    QPointer<QWindow> m_window;
    QTimer m_windowSave;
    QRect m_windowGeometry;
    bool m_windowMaximized = false;
    bool m_windowUnsaved = false;
    QHash<QObject*, QMetaObject::Connection> m_modals;
    QString m_selected;
    QStringList m_looked;
    QStringList m_staged;
    QStringList m_pins;
    QString m_canvas;
    bool m_canvasWide = false;
    qreal m_canvasWidth = 0;
    QString m_awaitedCanvas;
    QSet<QString> m_canvasesLeftOpen;
    QHash<QString, Computer> m_pinComputers;
    QString m_stageKey;
    QStringList m_restoring;
    QStringList m_savedStage;
    QStringList m_stageWritten;
    QHash<QString, Computer> m_computers;
    QList<Departure> m_departures;
    QSet<QString> m_closing;
    QStringList m_notedOutside;
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
    bool showsSomething(const QString& id) const;
    void followCanvas(const QString& id);
    void reconcileCanvas();
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
