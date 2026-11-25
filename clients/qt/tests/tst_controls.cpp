#include "app/QmlModelTypes.hpp"
#include "SessionFixture.hpp"
#include <QJsonDocument>
#include <QTemporaryDir>
#include <QAccessible>
#include <QGuiApplication>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQmlExpression>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtTest/QTest>
#include <QtQml/QQmlExtensionPlugin>

Q_IMPORT_QML_PLUGIN(KodosiPlugin)

#include <chrono>
#include <condition_variable>
#include <functional>
#include <memory>
#include <mutex>

namespace {
class PipeCommands final : public kodosi::CommandDispatcher, public kodosi::PipeRouter {
public:
    Result send(QByteArrayView bytes) override
    {
        std::lock_guard lock(m_mutex);
        m_sent.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        m_changed.notify_all();
        return {};
    }
    void route(const QString& pipe, QObject& reader, Read read) override
    {
        m_routes.insert(pipe, { &reader, std::move(read) });
    }
    void unroute(const QString& pipe) override { m_routes.remove(pipe); }
    void deliver(const QString& pipe, QJsonObject event)
    {
        event.insert(QStringLiteral("pipe"), pipe);
        const auto route = m_routes.value(pipe);
        QMetaObject::invokeMethod(
            route.reader, [read = route.read, event = QJsonDocument(event).toJson()] { read(event, false); },
            Qt::QueuedConnection);
    }
    void deliverOpened(const QString& pipe) { deliver(pipe, { { QStringLiteral("type"), QStringLiteral("document.opened") } }); }
    QStringList types() const
    {
        std::lock_guard lock(m_mutex);
        QStringList found;
        for (const auto& command : m_sent)
            found.append(command.value(QStringLiteral("type")).toString());
        return found;
    }
    QString lastPipe() const
    {
        std::lock_guard lock(m_mutex);
        return m_sent.last().value(QStringLiteral("pipe")).toString();
    }
    QList<QJsonObject> messages(const QString& pipe, const QString& type) const
    {
        std::lock_guard lock(m_mutex);
        return messagesLocked(pipe, type);
    }
    bool waitForMessage(const QString& pipe, const QString& type)
    {
        std::unique_lock lock(m_mutex);
        return m_changed.wait_for(
            lock, std::chrono::seconds(5), [&] { return !messagesLocked(pipe, type).isEmpty(); });
    }

private:
    struct Route {
        QObject* reader = nullptr;
        Read read;
    };
    QList<QJsonObject> messagesLocked(const QString& pipe, const QString& type) const
    {
        QList<QJsonObject> found;
        for (const auto& command : m_sent) {
            if (command.value(QStringLiteral("type")).toString() != QStringLiteral("document.send")
                || command.value(QStringLiteral("pipe")).toString() != pipe)
                continue;
            const auto bytes = QByteArray::fromBase64(command.value(QStringLiteral("data")).toString().toLatin1());
            const auto message = QJsonDocument::fromJson(bytes.mid(1)).object();
            if (message.value(QStringLiteral("type")).toString() == type)
                found.append(message);
        }
        return found;
    }
    mutable std::mutex m_mutex;
    std::condition_variable m_changed;
    QList<QJsonObject> m_sent;
    QHash<QString, Route> m_routes;
};

std::unique_ptr<QSettings> settingsIn(const QTemporaryDir& directory, const char* name)
{
    return std::make_unique<QSettings>(directory.filePath(QLatin1String(name)), QSettings::IniFormat);
}

class BrowserViews {
public:
    BrowserViews()
        : appearance(settingsIn(directory, "appearance.ini"),
              { [] { return Qt::ColorScheme::Dark; }, [](Qt::ColorScheme) {}, [] {} })
        , desktop(settingsIn(directory, "desktop.ini"))
        , settings(settingsIn(directory, "settings.ini"))
        , workspace(commands, sessions, desktop, settings)
        , canvas(commands, sessions)
        , web(canvas)
        , documents(commands, commands, sessions)
    {
        kodosi::qml::provide<kodosi::qml::AppearanceModelForeign>(engine, appearance);
        kodosi::qml::provide<kodosi::qml::DesktopStateModelForeign>(engine, desktop);
        kodosi::qml::provide<kodosi::qml::DesktopSettingsForeign>(engine, settings);
        kodosi::qml::provide<kodosi::qml::SessionCatalogModelForeign>(engine, sessions);
        kodosi::qml::provide<kodosi::qml::AppStateForeign>(engine, workspace);
        kodosi::qml::provide<kodosi::qml::AccountModelForeign>(engine, workspace.account());
        kodosi::qml::provide<kodosi::qml::PeopleModelForeign>(engine, workspace.people());
        kodosi::qml::provide<kodosi::qml::ProjectsModelForeign>(engine, workspace.projects());
        kodosi::qml::provide<kodosi::qml::CanvasModelForeign>(engine, canvas);
        kodosi::qml::provide<kodosi::qml::CanvasWebForeign>(engine, web);
        kodosi::qml::provide<kodosi::qml::CanvasDocumentsForeign>(engine, documents);
        kodosi::qml::provide<kodosi::qml::DesktopFileIntegrationForeign>(engine, files);
    }
    std::unique_ptr<QObject> view(const QString& sessionId)
    {
        return window(QStringLiteral(R"(CanvasBrowser { anchors.fill: parent; sessionId: "%1"; browser: ({ state: "awake" }) })")
                .arg(sessionId));
    }
    std::unique_ptr<QObject> panel(const QString& sessionId)
    {
        return window(QStringLiteral(R"(CanvasPanel { anchors.fill: parent; sessionId: "%1" })").arg(sessionId));
    }
    QStringList opensAndCloses() const
    {
        auto types = commands.types();
        types.removeIf([](const QString& type) {
            return type != QStringLiteral("document.open") && type != QStringLiteral("document.close");
        });
        return types;
    }
    QList<QJsonObject> watchesAfterOpening(const QString& pipe)
    {
        commands.deliverOpened(pipe);
        if (!QTest::qWaitFor([&] { return !commands.messages(pipe, QStringLiteral("watch")).isEmpty(); }))
            qWarning("the view sent no watch");
        return commands.messages(pipe, QStringLiteral("watch"));
    }

    PipeCommands commands;
    QTemporaryDir directory;
    kodosi::AppearanceModel appearance;
    kodosi::DesktopStateModel desktop;
    kodosi::DesktopSettings settings;
    kodosi::SessionCatalogModel sessions;
    kodosi::Workspace workspace;
    kodosi::CanvasModel canvas;
    kodosi::CanvasWeb web;
    kodosi::CanvasDocuments documents;
    kodosi::DesktopFileIntegration files;
    QQmlEngine engine;

private:
    std::unique_ptr<QObject> window(const QString& content)
    {
        QQmlComponent component(&engine);
        component.setData(QStringLiteral(R"(
            import QtQuick
            import Kodosi 1.0
            Window {
                width: 640; height: 480; visible: true
                %1
            }
        )").arg(content).toUtf8(), {});
        std::unique_ptr<QObject> made(component.create());
        if (!made)
            qWarning().noquote() << component.errorString();
        return made;
    }
};

QString terminal(int number)
{
    return QStringLiteral("0198aaaa-0000-7000-8000-%1").arg(number, 12, 10, QLatin1Char('0'));
}

QJsonObject terminalWithPictureAndBrowser()
{
    auto entry = test::session(1);
    entry.insert(QStringLiteral("canvas"),
        QJsonObject { { QStringLiteral("sequence"), 1 },
            { QStringLiteral("items"),
                QJsonArray { QJsonObject { { QStringLiteral("key"), QStringLiteral("/work/plan.png") },
                    { QStringLiteral("name"), QStringLiteral("plan.png") }, { QStringLiteral("kind"), QStringLiteral("picture") },
                    { QStringLiteral("revision"), 1 }, { QStringLiteral("bytes"), 120 }, { QStringLiteral("changed"), 1 } } } },
            { QStringLiteral("browser"),
                QJsonObject { { QStringLiteral("state"), QStringLiteral("awake") }, { QStringLiteral("changed"), 2 },
                    { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } } } });
    return entry;
}
}

class ControlsTest final : public QObject {
    Q_OBJECT
private slots:
    void terminalCloseConfirmsAndMinimizeOnlyHidesView();
    void terminalTilesAskBeforeAClipboardReadOnlyInAnotherPersonsTerminal();
    void leavingTerminalsLeavesNoDeferredWork();
    void messageTextKeepsSpacesEscapesMarkupAndSkipsYourself();
    void theSharedBrowserAcknowledgesWithoutTheGuiThreadAndComesBackWithoutThePage();
    void aPersonWhoChoosesAWatchedBrowserGivesThePageToItAtOnce();
    void aPersonWhoLeavesTheBrowserLeavesNoChoiceForALaterView();
    void aCanvasOpensAPipeToItsBrowserOnlyWhenItGoesToTheBrowser();
};

void ControlsTest::theSharedBrowserAcknowledgesWithoutTheGuiThreadAndComesBackWithoutThePage()
{
    BrowserViews rig;
    const auto view = rig.view(terminal(1));
    QVERIFY(view);
    QCOMPARE(rig.commands.types(), QStringList { QStringLiteral("document.open") });
    auto* theme = rig.engine.singletonInstance<QObject*>("Kodosi", "KodosiTheme");
    QVERIFY(theme);
    theme->setProperty("shown", false);
    QCOMPARE(rig.commands.types(), (QStringList { QStringLiteral("document.open"), QStringLiteral("document.close") }));
    theme->setProperty("shown", true);
    QCOMPARE(rig.commands.types().size(), 3);
    QCOMPARE(rig.commands.types().last(), QStringLiteral("document.open"));

    const auto back = rig.commands.lastPipe();
    const auto watches = rig.watchesAfterOpening(back);
    QCOMPARE(watches.size(), 1);
    QVERIFY(!watches.first().value(QStringLiteral("opened")).toBool());

    QByteArray frame(1, '\x01');
    frame += QByteArray::fromHex("0000000700") + QByteArray("delta");
    rig.commands.deliver(back, { { QStringLiteral("type"), QStringLiteral("document.frame") },
                                   { QStringLiteral("sequence"), 0 }, { QStringLiteral("more"), false },
                                   { QStringLiteral("data"), QString::fromLatin1(frame.toBase64()) } });
    QVERIFY(rig.commands.waitForMessage(back, QStringLiteral("ack")));
    QCOMPARE(rig.commands.messages(back, QStringLiteral("ack")).first().value(QStringLiteral("frame")).toInt(), 7);
}

void ControlsTest::aPersonWhoChoosesAWatchedBrowserGivesThePageToItAtOnce()
{
    BrowserViews rig;
    const auto view = rig.view(terminal(1));
    QVERIFY(view);
    const auto pipe = rig.commands.lastPipe();
    const auto watches = rig.watchesAfterOpening(pipe);
    QCOMPARE(watches.size(), 1);
    QVERIFY(!watches.first().value(QStringLiteral("opened")).toBool());
    rig.canvas.show(terminal(1), QStringLiteral("browser"));
    QTRY_COMPARE(rig.commands.messages(pipe, QStringLiteral("watch")).size(), 2);
    QVERIFY(rig.commands.messages(pipe, QStringLiteral("watch")).last().value(QStringLiteral("opened")).toBool());
}

void ControlsTest::aPersonWhoLeavesTheBrowserLeavesNoChoiceForALaterView()
{
    BrowserViews rig;
    const QList<std::function<void(const QString&)>> leaves {
        [&rig](const QString& id) { rig.canvas.show(id, QStringLiteral("files")); },
        [&rig](const QString& id) { rig.canvas.close(id, QStringLiteral("browser")); },
        [&rig](const QString& id) { rig.canvas.closeCanvas(id); },
    };
    for (int way = 0; way < leaves.size(); ++way) {
        const auto id = terminal(way + 1);
        rig.canvas.show(id, QStringLiteral("browser"));
        leaves.at(way)(id);
        const auto view = rig.view(id);
        QVERIFY(view);
        const auto watches = rig.watchesAfterOpening(rig.commands.lastPipe());
        QCOMPARE(watches.size(), 1);
        QVERIFY2(!watches.first().value(QStringLiteral("opened")).toBool(), qPrintable(QString::number(way)));
    }
}

void ControlsTest::aCanvasOpensAPipeToItsBrowserOnlyWhenItGoesToTheBrowser()
{
    BrowserViews rig;
    rig.sessions.apply(test::snapshot({ terminalWithPictureAndBrowser() }));
    rig.canvas.show(test::id(1), QStringLiteral("browser"));
    const auto panel = rig.panel(test::id(1));
    QVERIFY(panel);
    QCOMPARE(rig.opensAndCloses(), QStringList { QStringLiteral("document.open") });

    rig.canvas.pick(test::id(1), QStringLiteral("/work/plan.png"));
    QTRY_COMPARE(rig.opensAndCloses(), (QStringList { QStringLiteral("document.open"), QStringLiteral("document.close") }));
    rig.canvas.show(test::id(1), QStringLiteral("browser"));
    QCOMPARE(rig.opensAndCloses(),
        (QStringList { QStringLiteral("document.open"), QStringLiteral("document.close"), QStringLiteral("document.open") }));
}

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
    QQmlEngine engine;
    kodosi::qml::provide<kodosi::qml::AppearanceModelForeign>(engine, appearance);
    kodosi::qml::provide<kodosi::qml::DesktopStateModelForeign>(engine, desktop);
    kodosi::qml::provide<kodosi::qml::DesktopSettingsForeign>(engine, settings);
    kodosi::qml::provide<kodosi::qml::SessionCatalogModelForeign>(engine, sessions);
    kodosi::qml::provide<kodosi::qml::AppStateForeign>(engine, workspace);
    kodosi::qml::provide<kodosi::qml::AccountModelForeign>(engine, workspace.account());
    kodosi::qml::provide<kodosi::qml::PeopleModelForeign>(engine, workspace.people());
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import Kodosi 1.0
        QtObject {
            readonly property var targets: [
                { id: "u1", name: "Leo Park", kind: "person", handle: "leo" },
                { id: "u2", name: "Maya Chen", kind: "person" },
                { id: "t\"1", name: "Amber Wren", kind: "terminal", program: "" }
            ]
            readonly property var parts: Mentions.segments("a  <b> @maya chen\n  @Amber Wren a@Maya.com", targets)
            readonly property string html: Mentions.html(parts, "u1")
            readonly property string chips: Mentions.chips(parts, "u1").map(chip => chip.start + "-" + chip.end + " " + chip.color).join(",")
            readonly property string offered: Mentions.suggestions("@", targets, "u1").map(target => target.id).join(",")
            readonly property bool mine: Mentions.mentions(Mentions.segments("ping @Leo Park", targets), "u1")
            readonly property bool mineByName: Mentions.mentions(Mentions.segments("thanks @leo.", targets), "u1")
            readonly property bool notMine: Mentions.mentions(Mentions.segments("@leopard", targets), "u1")
        }
    )", {});
    std::unique_ptr<QObject> object(component.create());
    QVERIFY2(object, qPrintable(component.errorString()));
    const auto html = object->property("html").toString();
    QVERIFY2(html.startsWith(QStringLiteral("<span style=\"white-space:pre-wrap\">a  &lt;b&gt; <span style=\"color:#")), qPrintable(html));
    QVERIFY2(html.contains(QStringLiteral("\">@maya&nbsp;chen</span>\n  <a href=\"kodosi-mention://terminal/t&quot;1\"")), qPrintable(html));
    QVERIFY2(html.endsWith(QStringLiteral("\">@Amber&nbsp;Wren</a> a@Maya.com</span>")), qPrintable(html));
    QVERIFY2(!html.contains(QStringLiteral("background")), qPrintable(html));
    const auto chips = object->property("chips").toString();
    QVERIFY2(chips.startsWith(QStringLiteral("7-17 #")), qPrintable(chips));
    QVERIFY2(chips.endsWith(QStringLiteral(",20-31 #eaa47e")), qPrintable(chips));
    QCOMPARE(object->property("offered").toString(), QStringLiteral("u2,t\"1"));
    QVERIFY(object->property("mine").toBool());
    QVERIFY(object->property("mineByName").toBool());
    QVERIFY(!object->property("notMine").toBool());
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
    kodosi::CanvasModel canvas(commands, sessions);
    workspace.apply(test::snapshot({test::session(1), test::session(2, true, false)}), 0);
    QQmlEngine engine;
    kodosi::qml::provide<kodosi::qml::AppearanceModelForeign>(engine, appearance);
    kodosi::qml::provide<kodosi::qml::DesktopStateModelForeign>(engine, desktop);
    kodosi::qml::provide<kodosi::qml::DesktopSettingsForeign>(engine, settings);
    kodosi::qml::provide<kodosi::qml::SessionCatalogModelForeign>(engine, sessions);
    kodosi::qml::provide<kodosi::qml::AppStateForeign>(engine, workspace);
    kodosi::qml::provide<kodosi::qml::AccountModelForeign>(engine, workspace.account());
    kodosi::qml::provide<kodosi::qml::PeopleModelForeign>(engine, workspace.people());
    kodosi::qml::provide<kodosi::qml::DevicesModelForeign>(engine, workspace.devices());
    kodosi::qml::provide<kodosi::qml::ProjectsModelForeign>(engine, workspace.projects());
    kodosi::qml::provide<kodosi::qml::DosiModelForeign>(engine, workspace.dosi());
    kodosi::qml::provide<kodosi::qml::SessionActionsModelForeign>(engine, workspace.sessionActions());
    kodosi::qml::provide<kodosi::qml::TerminalSurfaceControllerForeign>(engine, surfaces);
    kodosi::qml::provide<kodosi::qml::CanvasModelForeign>(engine, canvas);
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls.Basic
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
    kodosi::CanvasModel canvas(commands, sessions);
    workspace.apply(test::snapshot({test::session(1)}), 0);
    QVERIFY(workspace.sessionActions().activate(test::id(1)));
    QQmlEngine engine;
    kodosi::qml::provide<kodosi::qml::AppearanceModelForeign>(engine, appearance);
    kodosi::qml::provide<kodosi::qml::DesktopStateModelForeign>(engine, desktop);
    kodosi::qml::provide<kodosi::qml::DesktopSettingsForeign>(engine, settings);
    kodosi::qml::provide<kodosi::qml::SessionCatalogModelForeign>(engine, sessions);
    kodosi::qml::provide<kodosi::qml::AppStateForeign>(engine, workspace);
    kodosi::qml::provide<kodosi::qml::AccountModelForeign>(engine, workspace.account());
    kodosi::qml::provide<kodosi::qml::PeopleModelForeign>(engine, workspace.people());
    kodosi::qml::provide<kodosi::qml::DevicesModelForeign>(engine, workspace.devices());
    kodosi::qml::provide<kodosi::qml::ProjectsModelForeign>(engine, workspace.projects());
    kodosi::qml::provide<kodosi::qml::DosiModelForeign>(engine, workspace.dosi());
    kodosi::qml::provide<kodosi::qml::SessionActionsModelForeign>(engine, workspace.sessionActions());
    kodosi::qml::provide<kodosi::qml::TerminalSurfaceControllerForeign>(engine, surfaces);
    kodosi::qml::provide<kodosi::qml::CanvasModelForeign>(engine, canvas);
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls.Basic
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
    auto* closeAccessible = QAccessible::queryAccessibleInterface(close);
    auto* minimizeAccessible = QAccessible::queryAccessibleInterface(minimize);
    QVERIFY(closeAccessible && minimizeAccessible);
    QVERIFY(!closeAccessible->text(QAccessible::Name).isEmpty());
    QVERIFY(!minimizeAccessible->text(QAccessible::Name).isEmpty());
    QVERIFY(QMetaObject::invokeMethod(close, "clicked"));
    QTRY_VERIFY(confirmation->property("visible").toBool());
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
    kodosi::CanvasModel canvas(commands, sessions);
    workspace.apply(test::snapshot({test::session(1)}), 0);
    QVERIFY(workspace.sessionActions().activate(test::id(1)));
    QQmlEngine engine;
    kodosi::qml::provide<kodosi::qml::AppearanceModelForeign>(engine, appearance);
    kodosi::qml::provide<kodosi::qml::DesktopStateModelForeign>(engine, desktop);
    kodosi::qml::provide<kodosi::qml::DesktopSettingsForeign>(engine, settings);
    kodosi::qml::provide<kodosi::qml::SessionCatalogModelForeign>(engine, sessions);
    kodosi::qml::provide<kodosi::qml::AppStateForeign>(engine, workspace);
    kodosi::qml::provide<kodosi::qml::AccountModelForeign>(engine, workspace.account());
    kodosi::qml::provide<kodosi::qml::PeopleModelForeign>(engine, workspace.people());
    kodosi::qml::provide<kodosi::qml::DevicesModelForeign>(engine, workspace.devices());
    kodosi::qml::provide<kodosi::qml::ProjectsModelForeign>(engine, workspace.projects());
    kodosi::qml::provide<kodosi::qml::DosiModelForeign>(engine, workspace.dosi());
    kodosi::qml::provide<kodosi::qml::SessionActionsModelForeign>(engine, workspace.sessionActions());
    kodosi::qml::provide<kodosi::qml::AgentLauncherModelForeign>(engine, launcher);
    kodosi::qml::provide<kodosi::qml::DesktopFileIntegrationForeign>(engine, files);
    kodosi::qml::provide<kodosi::qml::TerminalTilingLayoutModelForeign>(engine, tiling);
    kodosi::qml::provide<kodosi::qml::TerminalSurfaceControllerForeign>(engine, surfaces);
    kodosi::qml::provide<kodosi::qml::CanvasModelForeign>(engine, canvas);
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls.Basic
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
    QTest::failOnWarning();
    desktop.setActiveView(kodosi::DesktopStateModel::Settings);
    QTRY_VERIFY(!tile());
    QTest::qWait(50);
}

QTEST_MAIN(ControlsTest)
#include "tst_controls.moc"
