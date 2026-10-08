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
    void missionCreationHasOneNameField();
};

void ControlsTest::keyboardFocusTracksButtonState()
{
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
        QDBusConnection(QStringLiteral("control-test-no-bus")),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} }, false);
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
        QDBusConnection(QStringLiteral("modal-test-no-bus")),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} }, false);
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat), false);
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
        QDBusConnection(QStringLiteral("close-test-no-bus")),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} }, false);
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat), false);
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
    QQmlExpression actionText(QQmlEngine::contextForObject(confirmation), confirmation,
        QStringLiteral("standardButton(Dialog.Ok).text"));
    QTRY_COMPARE(actionText.evaluate().toString(), QStringLiteral("Close"));
    QVERIFY(!actionText.hasError());
    bool explainsProcessExit = false;
    for (auto* item : confirmation->findChildren<QQuickItem*>()) {
        if (item->property("text").toString() == QStringLiteral("Running programs will stop."))
            explainsProcessExit = true;
    }
    QVERIFY(explainsProcessExit);
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
        QDBusConnection(QStringLiteral("mission-test-no-bus")),
        { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} }, false);
    kodosi::DesktopStateModel desktop(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("desktop.ini")), QSettings::IniFormat), false);
    kodosi::DesktopSettings settings(
        std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
    kodosi::SessionCatalogModel sessions;
    desktop.attachSessionCatalog(&sessions);
    kodosi::Workspace workspace(commands, sessions, desktop, settings);
    kodosi::qml::AppearanceModelForeign::instance = &appearance;
    kodosi::qml::DesktopStateModelForeign::instance = &desktop;
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
