#include "app/AppActivation.hpp"
#include "app/ApplicationLifecycleModel.hpp"
#include "app/DeepLinkRouter.hpp"
#include "app/QmlModelTypes.hpp"
#include "canvas/CanvasImages.hpp"
#include "canvas/CanvasWeb.hpp"
#include "logging/QtLogForwarder.hpp"
#include "platform/TerminalNotifications.hpp"
#include "runtime/RuntimeBridge.hpp"
#include "presentation/AppearanceModel.hpp"
#include "presentation/CanvasModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/TerminalTilingLayoutModel.hpp"
#include "presentation/Workspace.hpp"
#include "platform/DesktopFileIntegration.hpp"
#include "platform/DesktopTray.hpp"
#include "platform/SystemNotificationDriver.hpp"
#include "terminal/TerminalAccessibility.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include <kodosi_runtime.h>
#include "terminal/TerminalView.hpp"
#include <QApplication>
#include <QMessageBox>
#include <QPushButton>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QRegularExpression>
#include <QSettings>
#include <QSystemTrayIcon>
#include <QWindow>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>
#include <memory>
#include <optional>
#include <utility>
#ifdef Q_OS_LINUX
#include <QFile>
#include <QFileSystemWatcher>
#include <sys/stat.h>
#endif
#ifdef Q_OS_WIN
#include "platform/windows/PackageUpdate.hpp"
#include "platform/windows/WindowFrame.hpp"
#include <QFont>
#include <QQuickWindow>
#endif

namespace {

class AppClaim final {
public:
    AppClaim() = default;
    AppClaim(const AppClaim&) = delete;
    AppClaim& operator=(const AppClaim&) = delete;
    ~AppClaim() { release(); }

    void release()
    {
        if (!std::exchange(m_released, true))
            kodosi_close_app();
    }

private:
    bool m_released = false;
};

QString startFailure()
{
    kodosi_start_failure_t failure {};
    return kodosi_last_start_failure(&failure) == 1 ? QString::fromUtf8(failure.message)
                                                    : QStringLiteral("Kodosi could not start.");
}

bool replacesEarlierApp()
{
    const auto version = QStringLiteral(KODOSI_VERSION);
    QMessageBox box;
    box.setIcon(QMessageBox::Question);
    box.setWindowTitle(QStringLiteral("Kodosi"));
    box.setText(QCoreApplication::translate("main", "An earlier version of Kodosi is still open."));
    box.setInformativeText(QCoreApplication::translate(
        "main", "Close it to use Kodosi %1. Its terminals keep running until you restart them.")
                               .arg(version));
    QPushButton* replace
        = box.addButton(QCoreApplication::translate("main", "Use Kodosi %1").arg(version), QMessageBox::AcceptRole);
    box.addButton(QCoreApplication::translate("main", "Keep the earlier one"), QMessageBox::RejectRole);
    box.setDefaultButton(replace);
    box.exec();
    return box.clickedButton() == static_cast<QAbstractButton*>(replace);
}

}

int main(int argc, char* argv[])
{
#ifdef Q_OS_LINUX
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORMTHEME"))
        qputenv("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
#endif
#ifdef Q_OS_WIN
    // AltGr then types characters instead of sending Ctrl+Alt shortcuts.
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORM"))
        qputenv("QT_QPA_PLATFORM", "windows:altgr");
#endif
    QQuickStyle::setStyle(QStringLiteral("Basic"));
    kodosi::CanvasWeb::registerScheme();
    QtWebEngineQuick::initialize();
    QApplication app(argc, argv);
#ifdef Q_OS_WIN
    QFont::insertSubstitutions(
        QStringLiteral("monospace"),
        {QStringLiteral("Cascadia Mono"), QStringLiteral("Consolas")});
#endif
    app.setQuitOnLastWindowClosed(false);
    QCoreApplication::setOrganizationName(QStringLiteral("Kodosi"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("kodosi.com"));
    QCoreApplication::setApplicationName(QStringLiteral("Kodosi"));
    QCoreApplication::setApplicationVersion(QStringLiteral(KODOSI_VERSION));
    const auto executable = QCoreApplication::applicationFilePath();
#ifdef Q_OS_LINUX
    // Wayland names the window by this entry, so the desktop shows Kodosi's icon and name.
    QGuiApplication::setDesktopFileName(QStringLiteral("com.kodosi.Kodosi"));
#endif
    std::optional<QSize> windowSize;
    kodosi::AppActivation launch { .route = {}, .token = kodosi::takeActivationToken(), .appVersion = {} };
    const auto arguments = app.arguments();
    for (int i = 1; i < arguments.size(); ++i) {
        if (arguments[i] == QStringLiteral("--window-size")) {
            if (i + 1 >= arguments.size())
                return EXIT_FAILURE;
            const auto match
                = QRegularExpression(QStringLiteral("^([0-9]+)x([0-9]+)$")).match(arguments[++i]);
            if (!match.hasMatch() || match.captured(1).toInt() < 820 || match.captured(2).toInt() < 560)
                return EXIT_FAILURE;
            windowSize = QSize(match.captured(1).toInt(), match.captured(2).toInt());
        } else if (!arguments[i].startsWith(QLatin1Char('-'))) {
            if (launch.route)
                return EXIT_FAILURE;
            launch.route = kodosi::DeepLinkRouter::parse(arguments[i]);
        }
    }

    kodosi::AppActivations activations;
    AppClaim claim;
    const char* preferences = nullptr;
    auto role = kodosi_prepare(&kodosi::AppActivations::receive, &activations, &preferences);
    if (role == KODOSI_APP_SECONDARY) {
        const auto json = launch.toJson();
        role = kodosi_forward_activation(
            reinterpret_cast<const uint8_t*>(json.constData()), static_cast<uintptr_t>(json.size()));
        if (role == KODOSI_APP_FORWARDED_TO_EARLIER) {
            if (!replacesEarlierApp())
                return EXIT_SUCCESS;
            role = kodosi_replace_earlier_app();
            if (role != KODOSI_APP_OWNER) {
                QMessageBox::warning(nullptr, QStringLiteral("Kodosi"), startFailure());
                return EXIT_FAILURE;
            }
        }
        if (role == KODOSI_APP_FORWARDED)
            return EXIT_SUCCESS;
    }
    if (role != KODOSI_APP_OWNER) {
        qCritical().noquote() << startFailure();
        return EXIT_FAILURE;
    }
    const kodosi::QtLogForwarder logs;
    const auto preferencesPath = preferences ? QString::fromUtf8(preferences) : QString();
    const auto settings = [&preferencesPath] {
        if (preferencesPath.isEmpty())
            return std::make_unique<QSettings>();
        auto isolated = std::make_unique<QSettings>(preferencesPath, QSettings::IniFormat);
        isolated->setFallbacksEnabled(false);
        return isolated;
    };
    kodosi::TerminalSessionRegistry terminalSessions;
    kodosi::RuntimeBridge runtime(terminalSessions);
    kodosi::ApplicationLifecycleModel lifecycle(runtime);
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop(settings());
    desktop.attachSessionCatalog(&sessions);
    kodosi::DesktopSettings desktopSettings(settings(), QSystemTrayIcon::isSystemTrayAvailable());
    kodosi::Workspace workspace(runtime, sessions, desktop, desktopSettings);
    kodosi::CanvasModel canvas(runtime, sessions);
    kodosi::CanvasWeb canvasWeb(canvas);
    kodosi::ConversationHistoryModel history(runtime);
    kodosi::ProviderFilesModel providerFiles(runtime);
    kodosi::AgentLauncherModel launcher(runtime);
    kodosi::AppearanceModel appearance(settings());
    kodosi::DesktopFileIntegration files;
    kodosi::TerminalTilingLayoutModel tiling;
    const auto followStage = [&] {
        tiling.setStagedSessionIds(desktop.stagedSessionIds());
        tiling.setSelectedSessionId(desktop.selectedSessionId());
        tiling.setLayoutMode(desktop.stageLayoutMode());
    };
    for (const auto signal : { &kodosi::DesktopStateModel::stagedSessionIdsChanged,
             &kodosi::DesktopStateModel::selectedSessionIdChanged, &kodosi::DesktopStateModel::stageLayoutModeChanged })
        QObject::connect(&desktop, signal, &tiling, followStage);
    kodosi::SystemNotificationDriver notifications;
    kodosi::TerminalNotifications terminalNotifications(sessions, notifications);
    kodosi::TerminalSurfaceController surfaces(terminalSessions, runtime, sessions);
    kodosi::DesktopTray tray(desktopSettings);
    kodosi::installTerminalAccessibility();
    workspace.sessionActions().setStartSize([&] {
        const auto* window = QGuiApplication::focusWindow();
        return kodosi::TerminalView::startSize(tiling.nextTerminalSize(), desktopSettings.fontFamily(),
            desktopSettings.fontSize(), desktopSettings.lineHeight(),
            window != nullptr ? window->devicePixelRatio() : qApp->devicePixelRatio());
    });

    QObject::connect(&runtime, &kodosi::RuntimeBridge::eventReceived, &app, [&](const QJsonObject& event, quint64 accountEpoch, const QByteArray& payload) {
        workspace.apply(event, accountEpoch);
        terminalNotifications.apply(event);
        const auto type = event.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("canvas.result") || type == QStringLiteral("canvas.error"))
            canvas.apply(event);
        if (type == QStringLiteral("provider.result") || type == QStringLiteral("provider.error")) {
            history.apply(event, payload);
            providerFiles.apply(event);
            launcher.apply(event);
        }
        if (type == QStringLiteral("folders.result") || type == QStringLiteral("folders.error"))
            launcher.apply(event);
        if (type == QStringLiteral("auth.ready") || type == QStringLiteral("auth.required")) {
            history.reset();
            providerFiles.reset();
        }
    });
    QObject::connect(&runtime, &kodosi::RuntimeBridge::eventError, &workspace, &kodosi::Workspace::setError);
    QObject::connect(&runtime, &kodosi::RuntimeBridge::runningChanged, &app, [&](bool running) {
        if (!running) {
            workspace.reset();
            canvas.reset();
            history.reset();
            providerFiles.reset();
            launcher.reset();
        }
    });
    QObject::connect(&workspace, &kodosi::Workspace::hostReady, &launcher, [&] { launcher.refresh(true); });
    QObject::connect(&canvas, &kodosi::CanvasModel::copyReady, &files, &kodosi::DesktopFileIntegration::saveCopy);
    QObject::connect(&canvas, &kodosi::CanvasModel::problem, &workspace, &kodosi::Workspace::setError);
    QObject::connect(&app, &QGuiApplication::applicationStateChanged, &workspace, [&](Qt::ApplicationState state) {
        if (state == Qt::ApplicationActive)
            workspace.missions().readWatched();
    });
    kodosi::qml::configureModelInstances(
        workspace, history, providerFiles, launcher, appearance, desktopSettings, desktop, sessions, tiling, files,
        lifecycle, surfaces, tray, canvas, canvasWeb);
    QQmlApplicationEngine engine;
    engine.addImageProvider(QStringLiteral("canvas"), new kodosi::CanvasImages(canvas));
    QObject::connect(
        &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
        [] { QCoreApplication::exit(EXIT_FAILURE); }, Qt::QueuedConnection);
    lifecycle.beginInitialStart();
    engine.loadFromModule(QStringLiteral("Kodosi"), QStringLiteral("Main"));
    if (engine.rootObjects().isEmpty())
        return EXIT_FAILURE;
    auto* window = qobject_cast<QWindow*>(engine.rootObjects().first());
    if (!window)
        return EXIT_FAILURE;
#ifdef Q_OS_WIN
    if (auto* quick = qobject_cast<QQuickWindow*>(window)) {
        auto* frame = new kodosi::WindowFrame(*quick);
        frame->setColor(quick->color());
        QObject::connect(quick, &QQuickWindow::colorChanged, frame, &kodosi::WindowFrame::setColor);
    }
#endif
    files.setTransientParent(window);
    desktop.attachWindow(window, windowSize);
    auto raise = [window](const QString& token) {
        window->setVisible(true);
        kodosi::withActivationToken(token, [window] {
            window->raise();
            window->requestActivate();
        });
    };
    const auto open = [&](const kodosi::AppActivation& activation) {
        raise(activation.token);
        if (!activation.appVersion.isEmpty())
            lifecycle.offerRestart(activation.appVersion, QStringLiteral(KODOSI_VERSION));
        if (!activation.route)
            return;
        if (activation.route->destination)
            workspace.route(*activation.route->destination);
        else
            workspace.setError(kodosi::DeepLinkRouter::userMessage(activation.route->error));
    };
    QObject::connect(&terminalNotifications, &kodosi::TerminalNotifications::sessionRequested, &app,
        [&](const QString& id, const QString& token) {
            if (workspace.sessionActions().activate(id))
                raise(token);
        });
    QObject::connect(&workspace, &kodosi::Workspace::showRequested, &app, [&](const QString& id) {
        if (workspace.sessionActions().activate(id))
            raise({});
    });
    QObject::connect(&tray, &kodosi::DesktopTray::openRequested, &app, [&] { raise({}); });
#ifdef Q_OS_LINUX
    const auto program = QFile::encodeName(executable);
    struct stat running {};
    ::stat(program.constData(), &running);
    QFileSystemWatcher update { QStringList { executable } };
    QObject::connect(&update, &QFileSystemWatcher::fileChanged, &lifecycle, [&lifecycle, program, running] {
        struct stat installed {};
        if (::stat(program.constData(), &installed) != 0 || installed.st_ino != running.st_ino)
            lifecycle.setUpdateInstalled();
    });
#endif
#ifdef Q_OS_WIN
    kodosi::PackageUpdate update;
    QObject::connect(&update, &kodosi::PackageUpdate::staged, &lifecycle,
        &kodosi::ApplicationLifecycleModel::setUpdateInstalled);
#endif
    open(launch);
    activations.setHandler(open);
    QObject::connect(&app, &QCoreApplication::aboutToQuit, &lifecycle, [&] { lifecycle.stop(); });
    const auto code = app.exec();
    if (lifecycle.restartRequested()) {
        claim.release();
#ifdef Q_OS_WIN
        if (kodosi::PackageUpdate::openAgainAfterExit())
            return code;
#endif
        QProcess::startDetached(executable, {});
    }
    return code;
}
