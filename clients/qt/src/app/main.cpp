#include "app/AppActivation.hpp"
#include "app/ApplicationLifecycleModel.hpp"
#include "app/DeepLinkRouter.hpp"
#include "app/QmlModelTypes.hpp"
#include "logging/QtLogForwarder.hpp"
#include "platform/TerminalNotifications.hpp"
#include "runtime/RuntimeBridge.hpp"
#include "presentation/AppearanceModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/TerminalTilingLayoutModel.hpp"
#include "presentation/Workspace.hpp"
#include "platform/DesktopFileIntegration.hpp"
#include "platform/SystemNotificationDriver.hpp"
#ifdef Q_OS_WIN
#include "platform/windows/WindowFrame.hpp"
#endif
#include "terminal/TerminalAccessibility.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include <kodosi_runtime.h>
#include <QApplication>
#include <QFont>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QRegularExpression>
#include <QSettings>
#include <QWindow>
#include <memory>
#include <optional>

namespace {

class AppClaim final {
public:
    AppClaim() = default;
    AppClaim(const AppClaim&) = delete;
    AppClaim& operator=(const AppClaim&) = delete;
    ~AppClaim() { kodosi_close_app(); }
};

QString startFailure()
{
    kodosi_start_failure_t failure {};
    return kodosi_last_start_failure(&failure) == 1 ? QString::fromUtf8(failure.message)
                                                    : QStringLiteral("Kodosi could not start.");
}

}

int main(int argc, char* argv[])
{
#ifdef Q_OS_LINUX
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORMTHEME"))
        qputenv("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
#endif
#ifdef Q_OS_WIN
    // AltGr arrives as its own key, so layouts that type characters with it
    // reach the terminal as text rather than as Ctrl+Alt shortcuts.
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORM"))
        qputenv("QT_QPA_PLATFORM", "windows:altgr");
#endif
    QQuickStyle::setStyle(QStringLiteral("Basic"));
    QApplication app(argc, argv);
#ifdef Q_OS_WIN
    // Windows has no generic monospace family, so text that asks for one
    // gets the fonts the terminal uses.
    QFont::insertSubstitutions(
        QStringLiteral("monospace"),
        {QStringLiteral("Cascadia Mono"), QStringLiteral("Consolas")});
#endif
    app.setQuitOnLastWindowClosed(false);
    QCoreApplication::setOrganizationName(QStringLiteral("Kodosi"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("kodosi.com"));
    QCoreApplication::setApplicationName(QStringLiteral("Kodosi"));
    QCoreApplication::setApplicationVersion(QStringLiteral(KODOSI_VERSION));
    std::optional<QSize> windowSize;
    kodosi::AppActivation launch { .route = {}, .token = kodosi::takeActivationToken() };
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
    const AppClaim claim;
    const char* preferences = nullptr;
    auto role = kodosi_prepare(&kodosi::AppActivations::receive, &activations, &preferences);
    if (role == KODOSI_APP_SECONDARY) {
        const auto json = launch.toJson();
        role = kodosi_forward_activation(
            reinterpret_cast<const uint8_t*>(json.constData()), static_cast<uintptr_t>(json.size()));
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
    kodosi::DesktopStateModel desktop(settings(), true);
    desktop.attachSessionCatalog(&sessions);
    kodosi::DesktopSettings desktopSettings(settings());
    kodosi::Workspace workspace(runtime, sessions, desktop, desktopSettings);
    kodosi::ConversationHistoryModel history(runtime);
    kodosi::ProviderFilesModel providerFiles(runtime);
    kodosi::AppearanceModel appearance(settings());
    kodosi::DesktopFileIntegration files;
    kodosi::TerminalTilingLayoutModel tiling;
    kodosi::SystemNotificationDriver notifications;
    kodosi::TerminalNotifications terminalNotifications(sessions, notifications);
    kodosi::TerminalSurfaceController surfaces(terminalSessions, runtime, sessions);
    kodosi::installTerminalAccessibility();

    QObject::connect(&runtime, &kodosi::RuntimeBridge::eventReceived, &app, [&](const QJsonObject& event, quint64 accountEpoch, const QByteArray& payload) {
        workspace.apply(event, accountEpoch);
        terminalNotifications.apply(event);
        const auto type = event.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("provider.reply") || type == QStringLiteral("provider.error")) {
            history.apply(event, payload);
            providerFiles.apply(event);
        }
        if (type == QStringLiteral("auth.ready") || type == QStringLiteral("auth.required")) {
            history.reset();
            providerFiles.reset();
        }
    });
    QObject::connect(&runtime, &kodosi::RuntimeBridge::eventError, &workspace, &kodosi::Workspace::setError);
    QObject::connect(&sessions, &kodosi::SessionCatalogModel::authoritativeSnapshotApplied, &desktopSettings, [&] {
        for (const auto& program : sessions.localPrograms())
            desktopSettings.learn(program);
    });
    QObject::connect(&runtime, &kodosi::RuntimeBridge::runningChanged, &app, [&](bool running) {
        if (!running) {
            workspace.reset();
            history.reset();
            providerFiles.reset();
        }
    });
    QObject::connect(&lifecycle, &kodosi::ApplicationLifecycleModel::runtimeGenerationReady, &workspace,
        [&](quint64) { workspace.sessionActions().refresh(); });
    kodosi::qml::configureModelInstances(
        workspace, history, providerFiles, appearance, desktopSettings, desktop, sessions, tiling, files,
        lifecycle, surfaces);
    QQmlApplicationEngine engine;
    QObject::connect(
        &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
        [] { QCoreApplication::exit(EXIT_FAILURE); }, Qt::QueuedConnection);
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
    open(launch);
    activations.setHandler(open);
    QObject::connect(&app, &QCoreApplication::aboutToQuit, &lifecycle, [&] { lifecycle.stop(); });
    lifecycle.scheduleInitialStart();
    return app.exec();
}
