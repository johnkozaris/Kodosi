#include "app/QmlModelTypes.hpp"
#include "SessionFixture.hpp"
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTemporaryDir>
#include <QAccessible>
#include "terminal/TerminalAccessibility.hpp"
#include <QGuiApplication>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQmlExpression>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtTest/QTest>
#include <QtQml/QQmlExtensionPlugin>

Q_IMPORT_QML_PLUGIN(KodosiPlugin)

#include <memory>

class ControlsTest final : public QObject {
    Q_OBJECT
private slots:
    void keyboardFocusTracksButtonState();
    void modalControlsSuppressNativeTerminalAccessibility();
    void terminalCloseConfirmsAndMinimizeOnlyHidesView();
    void terminalTilesAskBeforeAClipboardReadOnlyInAnotherPersonsTerminal();
    void clipboardReadAnswersOnceTheTerminalTakesInputAgain();
    void leavingTerminalsLeavesNoDeferredWork();
    void missionCreationHasOneNameField();
    void messageTextKeepsSpacesEscapesMarkupAndSkipsYourself();
};

void ControlsTest::messageTextKeepsSpacesEscapesMarkupAndSkipsYourself()
{
    class Commands final : public kodosi::CommandDispatcher {
    public:
        Result send(QByteArrayView) override { return {}; }
    } commands;
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::DesktopSettings settings(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
    kodosi::SessionCatalogModel sessions;
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::qml::DesktopSettingsForeign::instance = &settings;
    kodosi::qml::SessionCatalogModelForeign::instance = &sessions;
    kodosi::qml::AppStateForeign::instance = &workspace;
    kodosi::qml::AccountModelForeign::instance = &workspace.account();
    kodosi::qml::PeopleModelForeign::instance = &workspace.people();
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import Kodosi 1.0
        QtObject {
            readonly property var targets: [
                { id: "u1", name: "Leo Park", kind: "person" },
                { id: "u2", name: "Maya Chen", kind: "person" },
                { id: "t\"1", name: "Amber Wren", kind: "terminal", program: "" }
            ]
            readonly property var parts: Mentions.segments("a  <b> @maya chen\n  @Amber Wren a@Maya.com", targets)
            readonly property string html: Mentions.html(parts, "u1")
            readonly property string chips: Mentions.chips(parts, "u1").map(chip => chip.start + "-" + chip.end + " " + chip.color).join(",")
            readonly property string offered: Mentions.suggestions("@", targets, "u1").map(target => target.id).join(",")
            readonly property bool mine: Mentions.mentions(Mentions.segments("ping @Leo Park", targets), "u1")
        }
    )", {});
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    const auto html = object->property("html").toString();
    QVERIFY2(html.startsWith(QStringLiteral("<span style=\"white-space:pre-wrap\">a  &lt;b&gt; <span style=\"color:#")), qPrintable(html));
    QVERIFY2(html.contains(QStringLiteral(";font-weight:600;text-decoration:none\">@maya&nbsp;chen</span>\n  <a href=\"kodosi-mention://terminal/t&quot;1\"")), qPrintable(html));
    QVERIFY2(html.endsWith(QStringLiteral("\">@Amber&nbsp;Wren</a> a@Maya.com</span>")), qPrintable(html));
    QVERIFY2(!html.contains(QStringLiteral("background")), qPrintable(html));
    const auto chips = object->property("chips").toString();
    QVERIFY2(chips.startsWith(QStringLiteral("7-17 #")), qPrintable(chips));
    QVERIFY2(chips.endsWith(QStringLiteral(",20-31 #eaa47e")), qPrintable(chips));
    QCOMPARE(object->property("offered").toString(), QStringLiteral("u2,t\"1"));
    QVERIFY(object->property("mine").toBool());
}

void ControlsTest::keyboardFocusTracksButtonState()
{
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        ApplicationWindow {
            width: 640; height: 240; visible: true
            KButton {
                objectName: "test.button"
                anchors.centerIn: parent
                width: 450
                text: "Mission"
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root != nullptr, qPrintable(component.errorString()));
    auto* window = qobject_cast<QQuickWindow*>(root.get());
    QVERIFY(window);
    auto* button = root->findChild<QQuickItem*>(QStringLiteral("test.button"));
    QVERIFY(button);
    auto* indicator = button->findChild<QQuickItem*>(QStringLiteral("control.keyboardFocus"));
    QVERIFY(indicator);
    QVERIFY(!indicator->isVisible());
    button->forceActiveFocus(Qt::TabFocusReason);
    QTRY_VERIFY(button->hasActiveFocus());
    QTRY_VERIFY(indicator->isVisible());
    QCOMPARE(indicator->height(), 2.0);
    button->setEnabled(false);
    QTRY_VERIFY(!indicator->isVisible());
}

void ControlsTest::modalControlsSuppressNativeTerminalAccessibility()
{
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::installTerminalAccessibility();
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        import Kodosi.Models 1.0 as Models
        ApplicationWindow {
            width: 640; height: 480; visible: true
            Models.AccessibilityScope {
                anchors.fill: parent
                enabled: !Models.DesktopState.modalOpen
                suppressed: Models.DesktopState.modalOpen
                Models.TerminalView { objectName: "test.terminal"; anchors.fill: parent }
            }
            KPopover { objectName: "test.popup"; modal: true; width: 100; height: 100; parent: Overlay.overlay }
            KDialog { objectName: "test.dialog"; width: 100; height: 100 }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    auto* terminal = root->findChild<kodosi::TerminalView*>(QStringLiteral("test.terminal"));
    auto* popup = root->findChild<QObject*>(QStringLiteral("test.popup"));
    auto* dialog = root->findChild<QObject*>(QStringLiteral("test.dialog"));
    QVERIFY(terminal && popup && dialog);
    auto* accessible = QAccessible::queryAccessibleInterface(terminal);
    QVERIFY(accessible);
    QVERIFY(!accessible->text(QAccessible::Name).isEmpty());
    QVERIFY(QMetaObject::invokeMethod(popup, "open"));
    QTRY_VERIFY(desktop.modalOpen());
    QVERIFY(!terminal->isEnabled());
    QVERIFY(accessible->text(QAccessible::Name).isEmpty());
    QVERIFY(QMetaObject::invokeMethod(dialog, "open"));
    QVERIFY(QMetaObject::invokeMethod(popup, "close"));
    QTRY_VERIFY(!popup->property("visible").toBool());
    QVERIFY(desktop.modalOpen());
    QVERIFY(QMetaObject::invokeMethod(dialog, "close"));
    QTRY_VERIFY(!desktop.modalOpen());
    QVERIFY(terminal->isEnabled());
    QVERIFY(!accessible->text(QAccessible::Name).isEmpty());
}

void ControlsTest::clipboardReadAnswersOnceTheTerminalTakesInputAgain()
{
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        import Kodosi.Models 1.0 as Models
        ApplicationWindow {
            id: window
            property var answers: []
            width: 640; height: 480; visible: true
            Item {
                id: stage
                anchors.fill: parent
                enabled: !Models.DesktopState.modalOpen
            }
            ClipboardReadDialog {
                objectName: "test.clipboardRead"
                sessionName: "Copper Otter"
                onAnswered: allowed => window.answers = window.answers.concat([[allowed, stage.enabled]])
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    auto* dialog = root->findChild<QObject*>(QStringLiteral("test.clipboardRead"));
    QVERIFY(dialog);
    const auto answers = [&] { return root->property("answers").toList(); };

    QVERIFY(QMetaObject::invokeMethod(dialog, "open"));
    QTRY_VERIFY(desktop.modalOpen());
    QCOMPARE(dialog->property("title").toString(), QStringLiteral("Let a program read your clipboard?"));
    QVERIFY(QMetaObject::invokeMethod(dialog, "accept"));
    QTRY_COMPARE(answers().size(), 1);
    QCOMPARE(answers().at(0).toList(), (QVariantList {true, true}));

    QVERIFY(QMetaObject::invokeMethod(dialog, "open"));
    QTRY_VERIFY(desktop.modalOpen());
    QVERIFY(QMetaObject::invokeMethod(dialog, "reject"));
    QTRY_COMPARE(answers().size(), 2);
    QCOMPARE(answers().at(1).toList(), (QVariantList {false, true}));
}

void ControlsTest::terminalTilesAskBeforeAClipboardReadOnlyInAnotherPersonsTerminal()
{
    class Commands final : public kodosi::CommandDispatcher {
    public:
        Result send(QByteArrayView) override { return {}; }
    } commands;
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::DesktopSettings settings(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
    kodosi::SessionCatalogModel sessions;
    desktop.attachSessionCatalog(&sessions);
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::TerminalSessionRegistry registry;
    kodosi::RuntimeBridge runtime;
    kodosi::TerminalSurfaceController surfaces(registry, runtime, sessions);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::qml::DesktopSettingsForeign::instance = &settings;
    kodosi::qml::SessionCatalogModelForeign::instance = &sessions;
    kodosi::qml::AppStateForeign::instance = &workspace;
    kodosi::qml::AccountModelForeign::instance = &workspace.account();
    kodosi::qml::PeopleModelForeign::instance = &workspace.people();
    kodosi::qml::DevicesModelForeign::instance = &workspace.devices();
    kodosi::qml::MissionsModelForeign::instance = &workspace.missions();
    kodosi::qml::SessionActionsModelForeign::instance = &workspace.sessionActions();
    kodosi::qml::TerminalSurfaceControllerForeign::instance = &surfaces;
    workspace.apply(test::snapshot({test::session(1), test::session(2, true, false)}), 0);
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        ApplicationWindow {
            width: 720; height: 480; visible: true
            Row {
                anchors.fill: parent
                TerminalTile {
                    width: 360; height: 480
                    sessionId: "0198aaaa-0000-7000-8000-000000000001"
                }
                TerminalTile {
                    width: 360; height: 480
                    sessionId: "0198aaaa-0000-7000-8000-000000000002"
                }
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    const auto terminal = [&](int session) {
        return root->findChild<QObject*>(QStringLiteral("stage.tile.") + test::id(session) + QStringLiteral(".terminal"));
    };
    QVERIFY(terminal(1) && terminal(2));
    QCOMPARE(terminal(1)->property("asksBeforeClipboardRead").toBool(), false);
    QCOMPARE(terminal(2)->property("asksBeforeClipboardRead").toBool(), true);
}

void ControlsTest::terminalCloseConfirmsAndMinimizeOnlyHidesView()
{
    class Commands final : public kodosi::CommandDispatcher {
    public:
        QList<QJsonObject> values;
        Result send(QByteArrayView bytes) override
        {
            values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
            return {};
        }
    } commands;
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::DesktopSettings settings(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
    kodosi::SessionCatalogModel sessions;
    desktop.attachSessionCatalog(&sessions);
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::TerminalSessionRegistry registry;
    kodosi::RuntimeBridge runtime;
    kodosi::TerminalSurfaceController surfaces(registry, runtime, sessions);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::qml::DesktopSettingsForeign::instance = &settings;
    kodosi::qml::SessionCatalogModelForeign::instance = &sessions;
    kodosi::qml::AppStateForeign::instance = &workspace;
    kodosi::qml::AccountModelForeign::instance = &workspace.account();
    kodosi::qml::PeopleModelForeign::instance = &workspace.people();
    kodosi::qml::DevicesModelForeign::instance = &workspace.devices();
    kodosi::qml::MissionsModelForeign::instance = &workspace.missions();
    kodosi::qml::SessionActionsModelForeign::instance = &workspace.sessionActions();
    kodosi::qml::TerminalSurfaceControllerForeign::instance = &surfaces;
    workspace.apply(test::snapshot({test::session(1)}), 0);
    QVERIFY(workspace.sessionActions().activate(test::id(1)));
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        ApplicationWindow {
            width: 720; height: 480; visible: true
            TerminalTile {
                anchors.fill: parent
                sessionId: "0198aaaa-0000-7000-8000-000000000001"
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    const auto tileName = QStringLiteral("stage.tile.") + test::id(1);
    auto* close = root->findChild<QQuickItem*>(tileName + QStringLiteral(".close"));
    auto* minimize = root->findChild<QQuickItem*>(tileName + QStringLiteral(".minimize"));
    auto* confirmation = root->findChild<QObject*>(tileName + QStringLiteral(".closeConfirmation"));
    QVERIFY(close && minimize && confirmation);
    QVERIFY(close->isVisible() && close->isEnabled());
    QCOMPARE(close->property("glyph").toString(), QStringLiteral("close"));
    auto* closeAccessible = QAccessible::queryAccessibleInterface(close);
    auto* minimizeAccessible = QAccessible::queryAccessibleInterface(minimize);
    QVERIFY(closeAccessible && minimizeAccessible);
    QCOMPARE(closeAccessible->text(QAccessible::Name), QStringLiteral("Close terminal"));
    QCOMPARE(minimizeAccessible->text(QAccessible::Name), QStringLiteral("Minimize"));
    QVERIFY(QMetaObject::invokeMethod(close, "clicked"));
    QTRY_VERIFY(confirmation->property("visible").toBool());
    QCOMPARE(confirmation->property("title").toString(), QStringLiteral("Close Terminal?"));
    QVERIFY(desktop.modalOpen());
    QVERIFY(commands.values.isEmpty());
    QQuickItem* explanation = nullptr;
    for (auto* item : confirmation->findChildren<QQuickItem*>()) {
        if (item->property("text").toString() == QStringLiteral("Its programs stop."))
            explanation = item;
    }
    QVERIFY(explanation);
    QQmlExpression actionText(QQmlEngine::contextForObject(explanation), confirmation,
        QStringLiteral("standardButton(Dialog.Ok).text"));
    QTRY_COMPARE(actionText.evaluate().toString(), QStringLiteral("Close"));
    QVERIFY(!actionText.hasError());
    QVERIFY(QMetaObject::invokeMethod(confirmation, "reject"));
    QTRY_VERIFY(!desktop.modalOpen());
    QVERIFY(commands.values.isEmpty());
    QVERIFY(QMetaObject::invokeMethod(minimize, "clicked"));
    QVERIFY(desktop.stagedSessionIds().isEmpty());
    QVERIFY(sessions.containsSession(test::id(1)));
    QVERIFY(commands.values.isEmpty());
    QVERIFY(workspace.sessionActions().activate(test::id(1)));
    QVERIFY(QMetaObject::invokeMethod(close, "clicked"));
    QTRY_VERIFY(confirmation->property("visible").toBool());
    QVERIFY(QMetaObject::invokeMethod(confirmation, "accept"));
    QCOMPARE(commands.values.size(), 1);
    QCOMPARE(commands.values.first().value(QStringLiteral("type")).toString(), QStringLiteral("session.close"));
    QCOMPARE(commands.values.first().value(QStringLiteral("expectedRuntimeIncarnationId")).toString(), test::id(101));
    QVERIFY(sessions.containsSession(test::id(1)));
    workspace.apply(test::snapshot({}), 0);
    QVERIFY(desktop.stagedSessionIds().isEmpty());
    QVERIFY(!sessions.containsSession(test::id(1)));
}

void ControlsTest::leavingTerminalsLeavesNoDeferredWork()
{
    class Commands final : public kodosi::CommandDispatcher {
    public:
        Result send(QByteArrayView) override { return {}; }
    } commands;
    QTemporaryDir directory;
    const auto ini = [&directory](const char* name) {
        return std::make_unique<QSettings>(directory.filePath(QLatin1String(name)), QSettings::IniFormat);
    };
    kodosi::AppearanceModel appearance(ini("appearance.ini"),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(ini("desktop.ini"));
    kodosi::DesktopSettings settings(ini("settings.ini"));
    kodosi::SessionCatalogModel sessions;
    desktop.attachSessionCatalog(&sessions);
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::AgentLauncherModel launcher(commands);
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
    kodosi::TerminalSessionRegistry registry;
    kodosi::RuntimeBridge runtime;
    kodosi::TerminalSurfaceController surfaces(registry, runtime, sessions);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::qml::DesktopSettingsForeign::instance = &settings;
    kodosi::qml::SessionCatalogModelForeign::instance = &sessions;
    kodosi::qml::AppStateForeign::instance = &workspace;
    kodosi::qml::AccountModelForeign::instance = &workspace.account();
    kodosi::qml::PeopleModelForeign::instance = &workspace.people();
    kodosi::qml::DevicesModelForeign::instance = &workspace.devices();
    kodosi::qml::MissionsModelForeign::instance = &workspace.missions();
    kodosi::qml::SessionActionsModelForeign::instance = &workspace.sessionActions();
    kodosi::qml::AgentLauncherModelForeign::instance = &launcher;
    kodosi::qml::DesktopFileIntegrationForeign::instance = &files;
    kodosi::qml::TerminalTilingLayoutModelForeign::instance = &tiling;
    kodosi::qml::TerminalSurfaceControllerForeign::instance = &surfaces;
    workspace.apply(test::snapshot({test::session(1)}), 0);
    QVERIFY(workspace.sessionActions().activate(test::id(1)));
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        import Kodosi.Models 1.0 as Models
        ApplicationWindow {
            width: 720; height: 480; visible: true
            Loader {
                anchors.fill: parent
                active: Models.DesktopState.activeView === Models.DesktopState.Sessions
                sourceComponent: TerminalStage {}
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    const auto tile = [&root] {
        QList<QQuickItem*> pending {qobject_cast<QQuickWindow*>(root.get())->contentItem()};
        while (!pending.isEmpty()) {
            auto* item = pending.takeLast();
            if (item->objectName() == QStringLiteral("stage.tile.") + test::id(1))
                return true;
            pending.append(item->childItems());
        }
        return false;
    };
    QTRY_VERIFY(tile());
    QTest::qWait(50);
    QTest::failOnWarning(QRegularExpression(QStringLiteral(".")));
    desktop.setActiveView(kodosi::DesktopStateModel::Settings);
    QTRY_VERIFY(!tile());
    QTest::qWait(50);
}

void ControlsTest::missionCreationHasOneNameField()
{
    class Commands final : public kodosi::CommandDispatcher {
    public:
        QList<QJsonObject> values;
        Result send(QByteArrayView bytes) override
        {
            values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
            return {};
        }
    } commands;
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} });
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat));
    kodosi::DesktopSettings settings(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
    kodosi::SessionCatalogModel sessions;
    desktop.attachSessionCatalog(&sessions);
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
    kodosi::qml::DesktopSettingsForeign::instance = &settings;
    kodosi::qml::SessionCatalogModelForeign::instance = &sessions;
    kodosi::qml::AppStateForeign::instance = &workspace;
    kodosi::qml::AccountModelForeign::instance = &workspace.account();
    kodosi::qml::PeopleModelForeign::instance = &workspace.people();
    kodosi::qml::DevicesModelForeign::instance = &workspace.devices();
    kodosi::qml::MissionsModelForeign::instance = &workspace.missions();
    kodosi::qml::SessionActionsModelForeign::instance = &workspace.sessionActions();
    workspace.apply({ {QStringLiteral("type"), QStringLiteral("auth.ready")},
        {QStringLiteral("userId"), test::id(50)} }, 1);
    commands.values.clear();
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi 1.0
        ApplicationWindow {
            width: 720; height: 480; visible: true
            MissionsView { anchors.fill: parent }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root, qPrintable(component.errorString()));
    auto* button = root->findChild<QQuickItem*>(QStringLiteral("missions.create"));
    auto* dialog = root->findChild<QObject*>(QStringLiteral("room.create.popup"));
    auto* name = root->findChild<QQuickItem*>(QStringLiteral("room.create.name"));
    auto* submit = root->findChild<QQuickItem*>(QStringLiteral("room.create"));
    QVERIFY(button && dialog && name && submit);
    QVERIFY(button->isEnabled());
    QVERIFY(QMetaObject::invokeMethod(button, "clicked"));
    QTRY_VERIFY(dialog->property("visible").toBool());
    QCOMPARE(root->findChildren<QQuickItem*>(QRegularExpression(QStringLiteral("^room\\.create\\.name$"))).size(), 1);
    QVERIFY(name->setProperty("text", QStringLiteral("Release")));
    QVERIFY(QMetaObject::invokeMethod(submit, "clicked"));
    QCOMPARE(commands.values.size(), 1);
    QCOMPARE(commands.values.first().value(QStringLiteral("type")).toString(), QStringLiteral("mission.create"));
    QCOMPARE(commands.values.first().value(QStringLiteral("name")).toString(), QStringLiteral("Release"));
    QCOMPARE(commands.values.first().size(), 3);
    auto* warning = root->findChild<QQuickItem*>(QStringLiteral("panel.missions.truncated"));
    QVERIFY(warning);
    QVERIFY(!warning->isVisible());
    const QJsonObject invitation {{QStringLiteral("id"), test::id(12)},
        {QStringLiteral("missionId"), test::id(10)}, {QStringLiteral("missionName"), QStringLiteral("Project")}};
    QJsonObject snapshot {{QStringLiteral("type"), QStringLiteral("missions.snapshot")},
        {QStringLiteral("missions"), QJsonArray {}},
        {QStringLiteral("invitations"), QJsonArray {invitation}},
        {QStringLiteral("invitationsTruncated"), true}};
    workspace.apply(snapshot, 1);
    QTRY_VERIFY(warning->isVisible());
    QQuickItem* decline = nullptr;
    const auto findDecline = [&] {
        QList<QQuickItem*> pending {qobject_cast<QQuickWindow*>(root.get())->contentItem()};
        while (!pending.isEmpty()) {
            auto* item = pending.takeLast();
            if (item->objectName() == QStringLiteral("panel.missionsView.decline.") + test::id(12)) {
                decline = item;
                return true;
            }
            pending.append(item->childItems());
        }
        return false;
    };
    QTRY_VERIFY(findDecline());
    QTRY_VERIFY(decline->isVisible() && decline->isEnabled());
    QVERIFY(QMetaObject::invokeMethod(decline, "clicked"));
    QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("mission.invitation.reject"));
    snapshot.remove(QStringLiteral("invitationsTruncated"));
    workspace.apply(snapshot, 1);
    QTRY_VERIFY(!warning->isVisible());
}

QTEST_MAIN(ControlsTest)
#include "tst_controls.moc"
