#include "app/ApplicationLifecycleModel.hpp"
#include "app/DeepLinkRouter.hpp"
#include "app/QmlModelTypes.hpp"
#include "app/SingleInstanceGuard.hpp"
#include "app/RuntimeStorageBootstrap.hpp"
#include "platform/TerminalNotifications.hpp"
#include "runtime/RuntimeBridge.hpp"
#include "logging/ApplicationLogStore.hpp"
#include "presentation/AppearanceModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "presentation/TerminalTilingLayoutModel.hpp"
#include "presentation/Workspace.hpp"
#include "platform/DesktopFileIntegration.hpp"
#include "platform/FreedesktopNotificationDriver.hpp"
#include "terminal/TerminalAccessibility.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalSurfaceController.hpp"
#include <QApplication>
#include <QJsonDocument>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QRegularExpression>
#include <QWindow>
#include <optional>

int main(int argc, char* argv[])
{
    if (!qEnvironmentVariableIsSet("QT_QPA_PLATFORMTHEME"))
        qputenv("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
    QQuickStyle::setStyle(QStringLiteral("Basic"));
    QApplication app(argc, argv);
    app.setQuitOnLastWindowClosed(false);
    QCoreApplication::setOrganizationName(QStringLiteral("Kodosi"));
    QCoreApplication::setOrganizationDomain(QStringLiteral("kodosi.com"));
    QCoreApplication::setApplicationName(QStringLiteral("Kodosi"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));
    std::optional<QSize> windowSize;
    std::optional<kodosi::DeepLinkParseResult> route;
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
            if (route)
                return EXIT_FAILURE;
            route = kodosi::DeepLinkRouter::parse(arguments[i]);
        }
    }
    auto activation = kodosi::SingleInstanceGuard::activation(route);
    qunsetenv("XDG_ACTIVATION_TOKEN");
    const auto storage = kodosi::RuntimeStorageBootstrap::resolve();
    if (!storage) {
        qCritical().noquote() << storage.error();
        return EXIT_FAILURE;
    }
    const auto name = kodosi::SingleInstanceGuard::endpointNamespace();
    if (!name.valid()) {
        qCritical().noquote() << name.error;
        return EXIT_FAILURE;
    }
    kodosi::SingleInstanceGuard single;
    const auto singleResult
        = single.start(kodosi::SingleInstanceGuard::standardRuntimeDirectory(), name.value, activation);
    if (singleResult.state == kodosi::SingleInstanceGuard::StartState::Forwarded)
        return EXIT_SUCCESS;
    if (singleResult.state != kodosi::SingleInstanceGuard::StartState::Owner) {
        qCritical().noquote() << singleResult.error;
        return EXIT_FAILURE;
    }

    if (auto prepared = storage->prepare(); !prepared) {
        qCritical().noquote() << prepared.error();
        return EXIT_FAILURE;
    }
    kodosi::ApplicationLogStore logs({ .directory = storage->logDirectory });
    if (!logs.healthy()) qWarning().noquote() << logs.lastError();
    kodosi::TerminalSessionRegistry terminalSessions;
    kodosi::RuntimeBridge runtime(terminalSessions);
    kodosi::ApplicationLifecycleModel lifecycle(runtime);
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop(storage->settings(), true);
    desktop.attachSessionCatalog(&sessions);
    kodosi::DesktopSettings settings(storage->settings());
    kodosi::Workspace workspace(runtime, sessions, desktop, settings);
    kodosi::ConversationHistoryModel history(runtime);
    kodosi::ProviderFilesModel providerFiles(runtime);
    kodosi::AppearanceModel appearance(storage->settings());
    kodosi::DesktopFileIntegration files;
    kodosi::TerminalTilingLayoutModel tiling;
    kodosi::FreedesktopNotificationDriver notifications;
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
    QObject::connect(&runtime, &kodosi::RuntimeBridge::runningChanged, &app, [&](bool running) {
        if (!running) {
            workspace.reset();
            history.reset();
            providerFiles.reset();
        }
    });
    QObject::connect(&lifecycle, &kodosi::ApplicationLifecycleModel::runtimeGenerationReady, &workspace,
        [&](quint64) { workspace.sessionActions().refresh(); });
    const auto syncTheme = [&] {
        if (runtime.isRunning()) {
            const auto command
                = QJsonDocument(QJsonObject { { QStringLiteral("type"), QStringLiteral("system.setTheme") },
                                    { QStringLiteral("dark"), appearance.dark() } })
                      .toJson(QJsonDocument::Compact);
            (void)runtime.send(command);
        }
    };
    QObject::connect(&appearance, &kodosi::AppearanceModel::effectiveSchemeChanged, &app, syncTheme);
    QObject::connect(&runtime, &kodosi::RuntimeBridge::eventReceived, &app, [&](const QJsonObject& event) {
        if (event.value(QStringLiteral("type")).toString() == QStringLiteral("system.ready"))
            syncTheme();
    });
    kodosi::qml::configureModelInstances(
        workspace, history, providerFiles, appearance, settings, desktop, sessions, tiling, files,
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
    files.setTransientParent(window);
    desktop.attachWindow(window, windowSize);
    auto raise = [window](const QString& token) {
        window->setVisible(true);
        kodosi::SingleInstanceGuard::withActivationToken(token, [window] {
            window->raise();
            window->requestActivate();
        });
    };
    QObject::connect(&single, &kodosi::SingleInstanceGuard::activationReceived, &app, raise);
    QObject::connect(&single, &kodosi::SingleInstanceGuard::routeReceived, &app,
        [&](kodosi::DeepLinkDestination link, const QString& token) {
            raise(token);
            workspace.route(link);
        });
    QObject::connect(&single, &kodosi::SingleInstanceGuard::invalidRouteReceived, &app,
        [&](kodosi::DeepLinkParseError error, const QString& token) {
            raise(token);
            workspace.setError(kodosi::DeepLinkRouter::userMessage(error));
        });
    QObject::connect(&terminalNotifications, &kodosi::TerminalNotifications::sessionRequested, &app,
        [&](const QString& id, const QString& token) {
            if (workspace.sessionActions().activate(id))
                raise(token);
        });
    raise(activation.activationToken);
    if (route) {
        if (route->destination)
            workspace.route(*route->destination);
        else
            workspace.setError(kodosi::DeepLinkRouter::userMessage(route->error));
    }
    if (single.publishEndpoint().state != kodosi::SingleInstanceGuard::StartState::Owner)
        return EXIT_FAILURE;
    QObject::connect(&app, &QCoreApplication::aboutToQuit, &lifecycle, [&] { lifecycle.stop(); });
    lifecycle.scheduleInitialStart();
    return app.exec();
}
