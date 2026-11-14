#include "SessionFixture.hpp"
#include "presentation/CanvasModel.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QCoreApplication>
#include <QJsonArray>
#include <algorithm>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QWindow>
#include <QtTest/QTest>
#include <limits>

class Commands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    bool reject = false;
    Result send(QByteArrayView bytes) override
    {
        if (reject)
            return std::unexpected(kodosi::RuntimeFailure { 6, QStringLiteral("Busy") });
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
};
struct Fixture {
    QTemporaryDir dir;
    QString root;
    Commands commands;
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop {
        std::make_unique<QSettings>(root + QStringLiteral("/ui.ini"), QSettings::IniFormat)
    };
    kodosi::DesktopSettings settings {
        std::make_unique<QSettings>(root + QStringLiteral("/terminal.ini"), QSettings::IniFormat)
    };
    kodosi::Workspace workspace { commands, sessions, desktop, settings };
    explicit Fixture(const QString& place = {})
        : root(place.isEmpty() ? dir.path() : place)
    {
        desktop.attachSessionCatalog(&sessions);
    }
};
QJsonObject onMini(int value, int restoredFrom = 0, const QString& owner = test::id(50),
    const QString& host = QStringLiteral("mini"))
{
    auto entry = test::session(value, true);
    entry.insert(QStringLiteral("ownerUserId"), owner);
    entry.insert(QStringLiteral("hostDeviceId"), host);
    if (restoredFrom != 0)
        entry.insert(QStringLiteral("restoredFrom"), test::id(restoredFrom));
    return entry;
}
bool opened(const Commands& commands, const QString& id)
{
    return std::ranges::any_of(commands.values, [&](const QJsonObject& command) {
        return command.value(QStringLiteral("type")).toString() == QStringLiteral("session.openRemote")
            && command.value(QStringLiteral("sessionId")).toString() == id;
    });
}
QJsonObject accountEvent(const QString& userId, int epoch)
{
    QJsonObject event { { QStringLiteral("type"), userId.isEmpty() ? QStringLiteral("auth.required") : QStringLiteral("auth.ready") },
        { QStringLiteral("accountEpoch"), epoch } };
    if (!userId.isEmpty())
        event.insert(QStringLiteral("userId"), userId);
    return event;
}
QJsonObject lastOpen(const Commands& commands)
{
    for (auto command = commands.values.crbegin(); command != commands.values.crend(); ++command) {
        if (command->value(QStringLiteral("type")).toString() == QStringLiteral("session.openRemote"))
            return *command;
    }
    return {};
}
QJsonObject openFailure(const QJsonObject& open)
{
    return { { QStringLiteral("type"), QStringLiteral("session.error") },
        { QStringLiteral("operation"), QStringLiteral("session.openRemote") },
        { QStringLiteral("requestId"), open.value(QStringLiteral("requestId")) },
        { QStringLiteral("message"), QStringLiteral("Kodosi could not reach the terminal connection service.") } };
}
QJsonObject showing(int value, int at)
{
    auto entry = test::session(value);
    entry.insert(QStringLiteral("canvas"), QJsonObject { { QStringLiteral("sequence"), at },
        { QStringLiteral("items"), QJsonArray { QJsonObject { { QStringLiteral("key"), QStringLiteral("/work/plan.md") },
            { QStringLiteral("name"), QStringLiteral("plan.md") }, { QStringLiteral("kind"), QStringLiteral("page") },
            { QStringLiteral("revision"), 1 }, { QStringLiteral("bytes"), 120 }, { QStringLiteral("changed"), 1 } } } },
        { QStringLiteral("open"), QJsonObject { { QStringLiteral("key"), QStringLiteral("/work/plan.md") },
            { QStringLiteral("at"), at } } } });
    return entry;
}
QJsonObject planItem(int changed)
{
    return { { QStringLiteral("key"), QStringLiteral("/work/plan.md") }, { QStringLiteral("name"), QStringLiteral("plan.md") },
        { QStringLiteral("kind"), QStringLiteral("page") }, { QStringLiteral("revision"), 1 }, { QStringLiteral("bytes"), 6 },
        { QStringLiteral("changed"), changed } };
}
QJsonObject asleep(int sequence, const QJsonArray& items = {})
{
    auto entry = test::session(1);
    entry.insert(QStringLiteral("canvas"), QJsonObject { { QStringLiteral("sequence"), sequence }, { QStringLiteral("items"), items },
        { QStringLiteral("browser"), QJsonObject { { QStringLiteral("state"), QStringLiteral("asleep") },
            { QStringLiteral("changed"), 1 }, { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } } } });
    return entry;
}
QUrl planFile(const QTemporaryDir& dir)
{
    QFile file(dir.filePath(QStringLiteral("plan.md")));
    if (!file.open(QIODevice::WriteOnly) || file.write("# Plan") != 6)
        return {};
    return QUrl::fromLocalFile(file.fileName());
}
class WorkspaceTest final : public QObject {
    Q_OBJECT
private slots:
    void projectChangesPublishTheMatchingDraftAndIdentity()
    {
        Fixture f;
        f.workspace.projects().open(test::id(10));
        f.workspace.projects().setPresentation(QStringLiteral("message"), QStringLiteral("Project ten draft"));
        f.workspace.projects().open(test::id(11));
        QString observedProject, observedDraft;
        connect(&f.workspace.projects(), &kodosi::ProjectsModel::contentChanged, &f.workspace, [&] {
            observedProject = f.workspace.projects().selectedProjectId();
            observedDraft = f.workspace.projects().presentation().value(QStringLiteral("message")).toString();
        });
        f.workspace.projects().open(test::id(10));
        QCOMPARE(observedProject, test::id(10));
        QCOMPARE(observedDraft, QStringLiteral("Project ten draft"));
    }

    void aProjectSendKeepsEditsMadeBeforeItsAcknowledgement()
    {
        Fixture f;
        f.workspace.projects().open(test::id(10));
        f.workspace.projects().setPresentation(QStringLiteral("message"), QStringLiteral("First message"));
        f.workspace.projects().act({{QStringLiteral("type"), QStringLiteral("post")}, {QStringLiteral("text"), QStringLiteral("First message")}});
        const auto command = f.commands.values.last();
        f.workspace.projects().setPresentation(QStringLiteral("message"), QStringLiteral("Next draft"));
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("project.result")},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))},
            {QStringLiteral("operation"), QStringLiteral("project.act")},
            {QStringLiteral("projectId"), test::id(10)}, {QStringLiteral("action"), QStringLiteral("post")}}, 0);
        QCOMPARE(f.workspace.projects().presentation().value(QStringLiteral("message")).toString(), QStringLiteral("Next draft"));
        f.workspace.projects().open(test::id(11));
        f.workspace.projects().open(test::id(10));
        QCOMPARE(f.workspace.projects().presentation().value(QStringLiteral("message")).toString(), QStringLiteral("Next draft"));
    }

    void aProjectTerminalKeepsTheProjectVisible()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({test::session(1, true, false)}), 0);
        f.workspace.projects().open(test::id(10));
        f.desktop.setActiveView(kodosi::DesktopStateModel::Projects);
        QSignalSpy navigation(&f.desktop, &kodosi::DesktopStateModel::activeViewChanged);
        QVERIFY(f.workspace.sessionActions().activateInProject(test::id(1)));
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Projects);
        QCOMPARE(f.workspace.projects().presentation().value(QStringLiteral("terminal")).toString(), test::id(1));
        f.desktop.selectSession(test::id(1));
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Projects);
        QCOMPARE(navigation.count(), 0);
    }

    void aProjectKeepsItsCanvasOpenOnATerminalWithoutOne()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({test::session(1)}), 0);
        f.workspace.projects().open(test::id(10));
        f.workspace.projects().setPresentation(QStringLiteral("showsCanvas"), true);
        QVERIFY(f.workspace.sessionActions().activateInProject(test::id(1)));
        QVERIFY(f.workspace.projects().presentation().value(QStringLiteral("showsCanvas")).toBool());
    }

    void theCanvasOfTheTerminalInFrontOpensOnceAtWhatItsAgentShows()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto first = test::id(1);
        const auto second = test::id(2);
        const auto show = [&](int firstAt, int secondAt) {
            auto one = showing(1, firstAt);
            auto two = showing(2, secondAt);
            one.insert(QStringLiteral("projectId"), test::id(10));
            two.insert(QStringLiteral("projectId"), test::id(10));
            f.workspace.apply(test::snapshot({ one, two }), 0);
            QCoreApplication::sendPostedEvents();
        };
        const auto projectShowsCanvas = [&] {
            return f.workspace.projects().presentation().value(QStringLiteral("showsCanvas")).toBool();
        };
        show(2, 2);
        QVERIFY(f.desktop.selectSession(first));
        QVERIFY(f.desktop.selectSession(second));
        f.sessions.setVisible(first, true);
        QVERIFY(f.workspace.frontSessionId().isEmpty());
        f.sessions.setVisible(second, true);
        QCOMPARE(f.workspace.frontSessionId(), second);

        show(3, 2);
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        QVERIFY(f.desktop.selectSession(first));
        QTRY_COMPARE(f.desktop.canvasSessionId(), first);
        QCOMPARE(f.desktop.stageLayoutMode(), kodosi::DesktopStateModel::Focus);
        QCOMPARE(canvas.current(first), QStringLiteral("/work/plan.md"));

        f.desktop.hideCanvas();
        f.sessions.setVisible(first, false);
        show(4, 2);
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        f.sessions.setVisible(first, true);
        QTRY_COMPARE(f.desktop.canvasSessionId(), first);

        f.workspace.projects().open(test::id(10));
        QVERIFY(f.workspace.sessionActions().activateInProject(second));
        QCOMPARE(f.workspace.frontSessionId(), second);
        show(5, 2);
        QVERIFY(!projectShowsCanvas());
        show(5, 6);
        QVERIFY(projectShowsCanvas());
    }

    void aChangeOpensTheCanvasInFrontOnlyWhenThePersonDidNotCloseIt()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto id = test::id(1);
        const auto apply = [&](int openAt, int changeAt) {
            auto entry = showing(1, openAt);
            auto shown = entry.value(QStringLiteral("canvas")).toObject();
            shown.insert(QStringLiteral("change"),
                QJsonObject { { QStringLiteral("key"), QStringLiteral("/work/plan.md") }, { QStringLiteral("at"), changeAt } });
            entry.insert(QStringLiteral("canvas"), shown);
            f.workspace.apply(test::snapshot({ entry }), 0);
            QCoreApplication::sendPostedEvents();
        };
        apply(2, 2);
        QVERIFY(f.desktop.selectSession(id));
        f.sessions.setVisible(id, true);
        apply(2, 3);
        QTRY_COMPARE(f.desktop.canvasSessionId(), id);
        f.desktop.closeCanvas();
        apply(2, 4);
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        apply(5, 4);
        QTRY_COMPARE(f.desktop.canvasSessionId(), id);
        f.settings.setOpensCanvas(false);
        f.desktop.hideCanvas();
        apply(6, 7);
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
    }

    void theOnlyTerminalLeftOnTheStageComesToTheFrontAndOpensAtWhatItsAgentShowed()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto first = test::id(1);
        f.workspace.apply(test::snapshot({ showing(1, 2), test::session(2) }), 0);
        QVERIFY(f.desktop.selectSession(first));
        QVERIFY(f.desktop.selectSession(test::id(2)));
        f.sessions.setVisible(first, true);
        f.sessions.setVisible(test::id(2), true);
        f.workspace.apply(test::snapshot({ showing(1, 3), test::session(2) }), 0);
        QCoreApplication::sendPostedEvents();
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        f.workspace.apply(test::snapshot({ showing(1, 3) }), 0);
        QVERIFY(f.desktop.selectedSessionId().isEmpty());
        QCOMPARE(f.workspace.frontSessionId(), first);
        QTRY_COMPARE(f.desktop.canvasSessionId(), first);
        QCOMPARE(f.desktop.selectedSessionId(), first);
    }

    void aTerminalOfAnotherComputerOpensItsCanvasOnlyAtWhatItsAgentDidAfterTheStart()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto id = test::id(1);
        auto terminal = onMini(1);
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ terminal }), 1);
        QVERIFY(f.desktop.selectSession(id));
        f.sessions.setVisible(id, true);
        auto shown = showing(1, 1).value(QStringLiteral("canvas")).toObject();
        terminal.insert(QStringLiteral("canvas"), shown);
        f.workspace.apply(test::snapshot({ terminal }), 1);
        QCoreApplication::sendPostedEvents();
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        shown.insert(QStringLiteral("sequence"), 3);
        shown.insert(QStringLiteral("browser"), QJsonObject { { QStringLiteral("state"), QStringLiteral("starting") },
            { QStringLiteral("changed"), 3 }, { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } });
        shown.insert(QStringLiteral("open"),
            QJsonObject { { QStringLiteral("key"), QStringLiteral("browser") }, { QStringLiteral("at"), 3 } });
        terminal.insert(QStringLiteral("canvas"), shown);
        f.workspace.apply(test::snapshot({ terminal }), 1);
        QTRY_COMPARE(f.desktop.canvasSessionId(), id);
        QCOMPARE(canvas.current(id), QStringLiteral("browser"));
    }

    void aFileAddedToACanvasClosedHereOpensItAtTheFileWhenItComesAndTheBrowserStaysClosed()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto id = test::id(1);
        const QStringList closed { QStringLiteral("browser") };
        f.workspace.apply(test::snapshot({ asleep(1) }), 0);
        QVERIFY(f.desktop.selectSession(id));
        canvas.close(id, QStringLiteral("browser"));
        f.workspace.addFiles(id, { planFile(f.dir) });
        const auto upload = f.commands.values.last();
        QCOMPARE(upload.value(QStringLiteral("request")).toObject().value(QStringLiteral("type")).toString(), QStringLiteral("attach"));
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        QCOMPARE(canvas.closedSections(id), closed);
        f.workspace.apply(test::snapshot({ asleep(2, { planItem(2) }) }), 0);
        canvas.apply({ { QStringLiteral("type"), QStringLiteral("canvas.result") },
            { QStringLiteral("requestId"), upload.value(QStringLiteral("requestId")) },
            { QStringLiteral("answer"), QJsonObject { { QStringLiteral("answer"), QStringLiteral("attached") },
                { QStringLiteral("key"), QStringLiteral("/work/plan.md") } } } });
        QCOMPARE(f.desktop.canvasSessionId(), id);
        QCOMPARE(canvas.current(id), QStringLiteral("/work/plan.md"));
        QCOMPARE(canvas.closedSections(id), closed);
    }

    void aFileAddedInARoomOpensTheProjectsCanvasAtItsFilesAndLeavesTheStage()
    {
        Fixture f;
        kodosi::CanvasModel canvas(f.commands, f.sessions, 2);
        f.workspace.attachCanvas(canvas);
        f.desktop.attachCanvas(&canvas);
        const auto id = test::id(1);
        f.workspace.apply(test::snapshot({ asleep(1) }), 0);
        f.workspace.projects().open(test::id(10));
        QVERIFY(f.workspace.sessionActions().activateInProject(id));
        canvas.close(id, QStringLiteral("browser"));
        f.workspace.addFiles(id, { planFile(f.dir) });
        QVERIFY(f.workspace.projects().presentation().value(QStringLiteral("showsCanvas")).toBool());
        f.workspace.apply(test::snapshot({ asleep(2, { planItem(2) }) }), 0);
        QVERIFY(canvas.showsSomething(id));
        QCOMPARE(canvas.closedSections(id), QStringList { QStringLiteral("browser") });
        QVERIFY(f.desktop.canvasSessionId().isEmpty());
        QCOMPARE(f.desktop.stageLayoutMode(), kodosi::DesktopStateModel::Grid);
    }

    void updatingProjectTasksPreservesTheirVisibleOrder()
    {
        Fixture f;
        f.workspace.projects().open(test::id(10));
        const auto task = [](int id, int version) {
            return QJsonObject {{QStringLiteral("id"), test::id(id)}, {QStringLiteral("version"), version}};
        };
        const auto project = [&](const QJsonArray& tasks) {
            f.workspace.apply({{QStringLiteral("type"), QStringLiteral("project.snapshot")},
                {QStringLiteral("content"), QJsonObject {{QStringLiteral("projectId"), test::id(10)},
                    {QStringLiteral("tasks"), tasks}, {QStringLiteral("messages"), QJsonArray {}}}}}, 0);
        };
        project({task(1,1), task(2,1)});
        project({task(2,1), task(1,2)});
        const auto tasks = f.workspace.projects().content().value(QStringLiteral("tasks")).toList();
        QCOMPARE(tasks.size(), 2);
        QCOMPARE(tasks.first().toMap().value(QStringLiteral("id")).toString(), test::id(1));
        QCOMPARE(tasks.first().toMap().value(QStringLiteral("version")).toInt(), 2);
    }

    void aProjectIsNewUntilItsPersonSeesTheLatestMessage()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        auto& projects = f.workspace.projects();
        const auto catalog = [&](bool unread) {
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("projects.snapshot") },
                { QStringLiteral("projects"), QJsonArray { QJsonObject { { QStringLiteral("id"), test::id(10) },
                    { QStringLiteral("name"), QStringLiteral("Ten") }, { QStringLiteral("unread"), unread } } } },
                { QStringLiteral("invitations"), QJsonArray {} } }, 1);
        };
        const auto project = [&](int sequence) {
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.snapshot") },
                { QStringLiteral("content"), QJsonObject { { QStringLiteral("projectId"), test::id(10) }, { QStringLiteral("tasks"), QJsonArray {} },
                    { QStringLiteral("messages"), QJsonArray { QJsonObject { { QStringLiteral("id"), test::id(100 + sequence) },
                        { QStringLiteral("sequence"), sequence }, { QStringLiteral("authorId"), test::id(51) } } } } } } }, 1);
        };
        const auto seen = [&] {
            QList<qint64> sequences;
            for (const auto& command : std::as_const(f.commands.values)) {
                if (command.value(QStringLiteral("type")).toString() == QStringLiteral("project.seen"))
                    sequences.append(command.value(QStringLiteral("sequence")).toInteger());
            }
            return sequences;
        };
        bool front = false;
        projects.isFront = [&front] { return front; };
        catalog(true);
        project(5);
        QCOMPARE(projects.unreadProjects(), QStringList { test::id(10) });
        QVERIFY(seen().isEmpty());
        f.desktop.setActiveView(kodosi::DesktopStateModel::Projects);
        projects.open(test::id(10));
        QVERIFY(seen().isEmpty());
        front = true;
        projects.readWatched();
        QCOMPARE(seen(), QList<qint64> { 5 });
        catalog(false);
        QVERIFY(projects.unreadProjects().isEmpty());
        QCOMPARE(seen(), QList<qint64> { 5 });
        projects.setPresentation(QStringLiteral("followLatest"), false);
        project(6);
        QCOMPARE(seen(), QList<qint64> { 5 });
        projects.setPresentation(QStringLiteral("followLatest"), true);
        QCOMPARE(seen(), (QList<qint64> { 5, 6 }));
    }
    void aProjectCountsWhatArrivedOutOfView()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        auto& projects = f.workspace.projects();
        f.desktop.setActiveView(kodosi::DesktopStateModel::Projects);
        projects.open(test::id(10));
        int sequence = 0;
        QJsonArray tasks;
        QJsonArray repositories;
        const auto project = [&](int messages) {
            QJsonArray list;
            for (int index = 0; index < messages; ++index) {
                ++sequence;
                list.append(QJsonObject { { QStringLiteral("id"), test::id(100 + sequence) }, { QStringLiteral("sequence"), sequence },
                    { QStringLiteral("authorId"), test::id(51) } });
            }
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.snapshot") },
                { QStringLiteral("content"), QJsonObject { { QStringLiteral("projectId"), test::id(10) }, { QStringLiteral("tasks"), tasks },
                    { QStringLiteral("repositories"), repositories }, { QStringLiteral("messages"), list } } } }, 1);
        };
        const auto fresh = [&] { return projects.presentation().value(QStringLiteral("fresh")).toList(); };
        const auto unread = [&] { return projects.presentation().value(QStringLiteral("unread")).toInt(); };
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 1 } } };
        project(0);
        QVERIFY(fresh().isEmpty());
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 2 } } };
        project(0);
        QCOMPARE(fresh(), QVariantList { 1 });
        repositories = { QJsonObject { { QStringLiteral("id"), test::id(2) } } };
        project(0);
        QCOMPARE(fresh(), (QVariantList { 1, 2 }));
        projects.setPresentation(QStringLiteral("section"), 1);
        QCOMPARE(fresh(), QVariantList { 2 });
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 3 } } };
        project(0);
        QCOMPARE(fresh(), QVariantList { 2 });

        project(1);
        QCOMPARE(unread(), 0);
        projects.setPresentation(QStringLiteral("conversation"), false);
        project(2);
        project(1);
        QCOMPARE(unread(), 3);
        projects.setPresentation(QStringLiteral("conversation"), true);
        QCOMPARE(unread(), 0);
        projects.setPresentation(QStringLiteral("followLatest"), false);
        project(1);
        QCOMPARE(unread(), 1);
        projects.setPresentation(QStringLiteral("followLatest"), true);
        QCOMPARE(unread(), 0);
    }

    void aListThatSaysTheSameKeepsTheViews()
    {
        Fixture f;
        const auto shown = [](const QString& activity) {
            auto entry = test::session(1);
            entry.insert(QStringLiteral("working"), true);
            entry.insert(QStringLiteral("activity"), activity);
            return test::snapshot({ entry });
        };
        f.workspace.apply(shown(QStringLiteral("Reading the logs")), 0);
        QSignalSpy views(&f.sessions, &kodosi::SessionCatalogModel::folderGroupsChanged);
        QSignalSpy applied(&f.sessions, &kodosi::SessionCatalogModel::authoritativeSnapshotApplied);
        f.workspace.apply(shown(QStringLiteral("Reading the logs")), 0);
        QCOMPARE(views.count(), 0);
        QCOMPARE(applied.count(), 1);
        QCOMPARE(f.sessions.sessions().first().toMap().value(QStringLiteral("activity")).toString(), QStringLiteral("Reading the logs"));
        QVERIFY(f.sessions.sessions().first().toMap().value(QStringLiteral("working")).toBool());
        f.workspace.apply(shown(QStringLiteral("Writing the fix")), 0);
        QCOMPARE(views.count(), 1);
        QCOMPARE(f.sessions.sessions().first().toMap().value(QStringLiteral("activity")).toString(), QStringLiteral("Writing the fix"));
    }

    void terminalControlFollowsOneConnectionState()
    {
        Fixture f;
        auto connected = test::session(1, true, false);
        connected.insert(QStringLiteral("connectionState"), QStringLiteral("connected"));
        f.sessions.apply(test::snapshot({ connected }));
        const auto presentation = f.sessions.presentationSession(test::id(1));
        QVERIFY(presentation);
        QVERIFY(presentation->canControl);
        const auto fields = f.sessions.presentationForSession(test::id(1));
        QVERIFY(fields.value(QStringLiteral("canControl")).toBool());
        for (const auto* state : { "connecting", "offline", "blocked" }) {
            connected.insert(QStringLiteral("connectionState"), QLatin1String(state));
            f.sessions.apply(test::snapshot({ connected }));
            QVERIFY(!f.sessions.presentationSession(test::id(1))->canControl);
        }
        f.sessions.apply(test::snapshot({ test::session(1) }));
        QVERIFY(f.sessions.presentationSession(test::id(1))->canControl);
    }
    void aTerminalWhoseComputerIsAwayKeepsItsViewAndTakesNoTyping()
    {
        Fixture f;
        auto remote = test::session(1, true, false);
        for (const auto* status : { "reconnecting", "offline" }) {
            remote.insert(QStringLiteral("status"), QLatin1String(status));
            f.sessions.apply(test::snapshot({ remote }));
            const auto presentation = f.sessions.presentationSession(test::id(1));
            QVERIFY(presentation->canControl);
            QVERIFY(!presentation->canType);
            QVERIFY(!f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("canControl")).toBool());
        }
        QVERIFY(f.sessions.presentationSession(test::id(1))->away);
    }
    void aHiddenTerminalLeavesTheListsAndItsViewUntilItIsBackButStaysInItsProject()
    {
        Fixture f;
        auto away = test::session(1, true, false);
        away.insert(QStringLiteral("status"), QStringLiteral("offline"));
        away.insert(QStringLiteral("hostName"), QStringLiteral("studio"));
        away.insert(QStringLiteral("projectId"), test::id(10));
        auto other = away;
        other.insert(QStringLiteral("id"), test::id(2));
        other.insert(QStringLiteral("incarnationId"), test::id(102));
        f.workspace.apply(test::snapshot({ away, other }), 0);
        const auto group = f.sessions.folderGroups().first().toMap();
        QVERIFY(group.value(QStringLiteral("offline")).toBool());
        QVERIFY(f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("canClose")).toBool());
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().hide(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.hide"));

        away.insert(QStringLiteral("hidden"), true);
        f.workspace.apply(test::snapshot({ away, other }), 0);
        QVERIFY(!f.desktop.stagedSessionIds().contains(test::id(1)));
        QCOMPARE(f.sessions.listedSessions().size(), 1);
        QCOMPARE(f.sessions.sessions().size(), 2);
        QCOMPARE(f.sessions.folderGroups().first().toMap().value(QStringLiteral("sessions")).toList().size(), 1);
        away.insert(QStringLiteral("hidden"), false);
        away.insert(QStringLiteral("status"), QStringLiteral("running"));
        f.workspace.apply(test::snapshot({ away, other }), 0);
        QCOMPARE(f.sessions.listedSessions().size(), 2);
        QCOMPARE(f.sessions.folderGroups().first().toMap().value(QStringLiteral("sessions")).toList().size(), 2);
        away.insert(QStringLiteral("hidden"), QStringLiteral("yes"));
        f.workspace.apply(test::snapshot({ away, other }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void folderGroupsKeepNamesAndRemoteDirectoriesPresentationOnly()
    {
        Fixture f;
        auto local = test::session(1);
        local.insert(QStringLiteral("workingDir"), QStringLiteral("/repo/project"));
        local.insert(QStringLiteral("activity"), QStringLiteral("GitHub Copilot"));
        auto remote = test::session(2, true, false);
        remote.insert(QStringLiteral("workingDir"), QStringLiteral("/host/project"));
        remote.insert(QStringLiteral("ownerName"), QStringLiteral("Ana"));
        remote.insert(QStringLiteral("hostName"), QStringLiteral("Studio"));
        auto unopened = remote;
        unopened.insert(QStringLiteral("id"), test::id(3));
        unopened.insert(QStringLiteral("incarnationId"), test::id(103));
        unopened.remove(QStringLiteral("workingDir"));
        auto mine = test::session(4, true, true);
        mine.insert(QStringLiteral("workingDir"), QStringLiteral("/home/me/website"));
        mine.insert(QStringLiteral("hostName"), QStringLiteral("Mini"));
        mine.insert(QStringLiteral("hostDeviceId"), QStringLiteral("mini"));
        f.sessions.apply(test::snapshot({ unopened, remote, mine, local }));
        QStringList labels;
        for (const auto& group : f.sessions.folderGroups()) {
            const auto fields = group.toMap();
            labels.append(QStringList { fields.value(QStringLiteral("computerTitle")).toString(),
                fields.value(QStringLiteral("computerStart")).toBool() ? QStringLiteral("start") : QString {},
                fields.value(QStringLiteral("name")).toString(),
                fields.value(QStringLiteral("canStart")).toBool() ? fields.value(QStringLiteral("directory")).toString() : QString {},
                fields.value(QStringLiteral("computerId")).toString() }.join(QLatin1Char('|')));
        }
        QCOMPARE(labels, (QStringList { QStringLiteral("This computer|start|project|/repo/project|"),
            QStringLiteral("Mini|start|website|/home/me/website|mini"), QStringLiteral("Ana|start|project||"),
            QStringLiteral("Ana||||") }));
        const auto fields = f.sessions.presentationForSession(test::id(1));
        QCOMPARE(fields.value(QStringLiteral("name")).toString(), QStringLiteral("Terminal"));
        QCOMPARE(fields.value(QStringLiteral("activity")).toString(), QStringLiteral("GitHub Copilot"));
        const auto hosted = f.sessions.presentationForSession(test::id(2));
        QVERIFY(hosted.value(QStringLiteral("workingDirectory")).toString().isEmpty());
        QCOMPARE(hosted.value(QStringLiteral("displayDirectory")).toString(), QStringLiteral("/host/project"));
    }
    void aFriendWithoutANameGetsNoNewTerminalButton()
    {
        Fixture f;
        auto shared = test::session(1, true, false);
        shared.insert(QStringLiteral("workingDir"), QStringLiteral("/home/ben/project"));
        shared.insert(QStringLiteral("ownerUserId"), test::id(50));
        shared.insert(QStringLiteral("hostDeviceId"), QStringLiteral("laptop"));
        shared.insert(QStringLiteral("hostName"), QStringLiteral("Laptop"));
        f.sessions.apply(test::snapshot({ shared }));
        const auto group = f.sessions.folderGroups().first().toMap();
        QVERIFY(!group.value(QStringLiteral("canStart")).toBool());
        QCOMPARE(group.value(QStringLiteral("owner")).toString(), QStringLiteral("Shared"));
        QCOMPARE(group.value(QStringLiteral("computerTitle")).toString(), QStringLiteral("Shared"));
    }
    void eachHostFolderShowsItsLastFolder()
    {
        const QList<std::pair<QString, QString>> folders {
            { QStringLiteral("/home/ada/project"), QStringLiteral("project") },
            { QStringLiteral("/home/ada/project/"), QStringLiteral("project") },
            { QStringLiteral("/home/ada/one\\two"), QStringLiteral("one\\two") },
            { QStringLiteral("/"), QStringLiteral("/") },
            { QStringLiteral("C:\\Users\\ada\\project"), QStringLiteral("project") },
            { QStringLiteral("d:/Users/ada/project\\"), QStringLiteral("project") },
            { QStringLiteral("C:\\"), QStringLiteral("C:") },
            { QStringLiteral("\\\\server\\share\\project"), QStringLiteral("project") },
            { QStringLiteral("\\\\server\\share"), QStringLiteral("share") },
        };
        for (const auto& [path, name] : folders)
            QCOMPARE(kodosi::SessionCatalogModel::folderName(path), name);
        Fixture f;
        auto remote = test::session(1, true, false);
        remote.insert(QStringLiteral("workingDir"), QStringLiteral("C:\\Users\\ada\\project"));
        f.sessions.apply(test::snapshot({ remote }));
        QCOMPARE(f.sessions.folderGroups().first().toMap().value(QStringLiteral("name")).toString(), QStringLiteral("project"));
        QCOMPARE(f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("folderName")).toString(),
            QStringLiteral("project"));
    }
    void aLocalWindowsFolderReachesQmlWithSlashes()
    {
        Fixture f;
        auto windows = test::session(1);
        windows.insert(QStringLiteral("workingDir"), QStringLiteral("C:\\Users\\ada\\project"));
        auto posix = test::session(2);
        posix.insert(QStringLiteral("workingDir"), QStringLiteral("/home/ada/one\\two"));
        f.sessions.apply(test::snapshot({ windows, posix }));
        const auto fields = f.sessions.presentationForSession(test::id(1));
        QCOMPARE(fields.value(QStringLiteral("workingDirectory")).toString(), QStringLiteral("C:/Users/ada/project"));
        QCOMPARE(fields.value(QStringLiteral("displayDirectory")).toString(), QStringLiteral("C:\\Users\\ada\\project"));
        QCOMPARE(f.sessions.presentationForSession(test::id(2)).value(QStringLiteral("workingDirectory")).toString(),
            QStringLiteral("/home/ada/one\\two"));
        QStringList directories;
        for (const auto& group : f.sessions.folderGroups())
            directories.append(group.toMap().value(QStringLiteral("directory")).toString());
        directories.sort();
        QCOMPARE(directories, (QStringList { QStringLiteral("/home/ada/one\\two"), QStringLiteral("C:/Users/ada/project") }));
    }
    void remoteActivationAndClose()
    {
        Fixture f;
        auto remote = test::session(1, true, false);
        remote.insert(QStringLiteral("connectionState"), QStringLiteral("offline"));
        f.workspace.apply(test::snapshot({ remote }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("session.openRemote"));
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));
        QVERIFY(f.workspace.sessionActions().close(test::id(1)));
        const auto command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("session.close"));
        QCOMPARE(command.value(QStringLiteral("expectedRuntimeIncarnationId")).toString(), test::id(101));
        QVERIFY(f.workspace.sessionActions().minimize(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),QStringLiteral("session.disconnect"));
        QVERIFY(f.sessions.containsSession(test::id(1)));
        QVERIFY(!f.workspace.sessionActions().share(test::id(1), {}));
    }
    void connectedRemoteActivationAcquiresThisClientsDemand()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({test::session(1, true, false)}), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.openRemote"));
        const auto count = f.commands.values.size();
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QCOMPARE(f.commands.values.size(), count);
        QVERIFY(f.workspace.sessionActions().minimize(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.disconnect"));
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.openRemote"));
        QCOMPARE(f.commands.values.size(), count + 2);
    }
    void theHostMarksTerminalsThatNeedYouAndHearsWhatYouSee()
    {
        Fixture f;
        auto marked = test::session(1);
        marked.insert(QStringLiteral("attention"), true);
        f.workspace.apply(test::snapshot({ marked, test::session(2) }), 0);
        QCOMPARE(f.sessions.attention(), QStringList { test::id(1) });
        f.sessions.setVisible(test::id(2), true);
        QVERIFY(f.commands.values.isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("system.ready") } }, 0);
        const auto visible = [&] { return f.commands.values.last().value(QStringLiteral("sessionIds")).toArray(); };
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.visible"));
        QCOMPARE(visible(), QJsonArray { test::id(2) });
        QCOMPARE(f.commands.values.last().value(QStringLiteral("watching")), QJsonValue(false));
        f.sessions.setVisible(test::id(1), true);
        f.sessions.setVisible(test::id(1), true);
        QCOMPARE(visible(), (QJsonArray { test::id(1), test::id(2) }));
        const auto sent = f.commands.values.size();
        f.sessions.setVisible(test::id(1), false);
        QCOMPARE(f.commands.values.size(), sent);
        f.sessions.setVisible(test::id(1), false);
        QCOMPARE(visible(), QJsonArray { test::id(2) });
        f.workspace.apply(test::snapshot({ test::session(1), test::session(2) }), 0);
        QVERIFY(f.sessions.attention().isEmpty());
    }
    void theHostHearsAtOnceWhenNoTerminalShows()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("system.ready") } }, 0);
        const auto report = f.commands.values.last();
        QCOMPARE(report.value(QStringLiteral("type")).toString(), QStringLiteral("session.visible"));
        QCOMPARE(report.value(QStringLiteral("sessionIds")).toArray(), QJsonArray {});
        QCOMPARE(report.value(QStringLiteral("watching")), QJsonValue(false));
    }
    void onlyALocalTerminalAtItsPromptOffersToStartAProgram()
    {
        Fixture f;
        auto local = test::session(1);
        local.insert(QStringLiteral("prompt"), true);
        auto busy = test::session(2);
        busy.insert(QStringLiteral("prompt"), false);
        auto remote = test::session(3, true);
        remote.insert(QStringLiteral("prompt"), true);
        f.workspace.apply(test::snapshot({ local, busy, remote, test::session(4) }), 0);
        const auto atPrompt = [&](const int number) {
            return f.sessions.presentationForSession(test::id(number)).value(QStringLiteral("atPrompt")).toBool();
        };
        QVERIFY(atPrompt(1));
        QVERIFY(!atPrompt(2));
        QVERIFY(!atPrompt(3));
        QVERIFY(!atPrompt(4));
    }
    void aTerminalShowsTheAccountPlaceOfItsAgent()
    {
        Fixture f;
        auto third = test::session(1);
        third.insert(QStringLiteral("program"), QStringLiteral("claude"));
        third.insert(QStringLiteral("programAccount"), 2);
        auto unknown = test::session(2);
        unknown.insert(QStringLiteral("program"), QStringLiteral("claude"));
        f.workspace.apply(test::snapshot({ third, unknown }), 0);
        const auto place = [&](const int number) {
            return f.sessions.presentationForSession(test::id(number)).value(QStringLiteral("programAccount")).toInt();
        };
        QCOMPARE(place(1), 2);
        QCOMPARE(place(2), -1);
        auto invalid = test::session(3);
        invalid.insert(QStringLiteral("programAccount"), 256);
        f.workspace.apply(test::snapshot({ invalid }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void aLocalTerminalSaysThatItsAgentStartedByHandDoesNotSeeKodosiUntilThePersonDismissesIt()
    {
        Fixture f;
        auto outside = test::session(1);
        outside.insert(QStringLiteral("program"), QStringLiteral("codex"));
        outside.insert(QStringLiteral("programOutside"), true);
        auto remote = test::session(2, true);
        remote.insert(QStringLiteral("program"), QStringLiteral("codex"));
        remote.insert(QStringLiteral("programOutside"), true);
        f.workspace.apply(test::snapshot({ outside, remote, test::session(3) }), 0);
        const auto noted = [&](const int number) {
            return f.sessions.presentationForSession(test::id(number)).value(QStringLiteral("programOutside")).toBool();
        };
        QVERIFY(noted(1));
        QVERIFY(!noted(2));
        QVERIFY(!noted(3));
        f.desktop.noteOutside(test::id(1));
        QCOMPARE(f.desktop.notedOutside(), QStringList { test::id(1) });
        f.workspace.apply(test::snapshot({ outside, remote }), 0);
        QCOMPARE(f.desktop.notedOutside(), QStringList { test::id(1) });
        f.workspace.apply(test::snapshot({ remote }), 0);
        QVERIFY(f.desktop.notedOutside().isEmpty());
        auto invalid = test::session(4);
        invalid.insert(QStringLiteral("programOutside"), QStringLiteral("yes"));
        f.workspace.apply(test::snapshot({ invalid }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void aRunningAgentShowsWhatItCanChangeAndAsksItsHostToChangeIt()
    {
        Fixture f;
        auto running = test::session(1);
        running.insert(QStringLiteral("program"), QStringLiteral("codex"));
        running.insert(QStringLiteral("programSettings"),
            QJsonObject { { QStringLiteral("model"), QStringLiteral("gpt-6") }, { QStringLiteral("permissions"), QStringLiteral("unrestricted") } });
        running.insert(QStringLiteral("programPending"), QJsonObject { { QStringLiteral("effort"), QStringLiteral("high") } });
        running.insert(QStringLiteral("programLive"),
            QJsonObject { { QStringLiteral("model"), true }, { QStringLiteral("effort"), true }, { QStringLiteral("keeps"), false },
                { QStringLiteral("permissions"), QJsonArray { QStringLiteral("ask"), QStringLiteral("auto") } } });
        f.workspace.apply(test::snapshot({ running }), 0);
        const auto shown = f.sessions.presentationForSession(test::id(1));
        QCOMPARE(shown.value(QStringLiteral("programSettings")).toMap().value(QStringLiteral("model")).toString(), QStringLiteral("gpt-6"));
        QCOMPARE(shown.value(QStringLiteral("programPending")).toMap().value(QStringLiteral("effort")).toString(), QStringLiteral("high"));
        QCOMPARE(shown.value(QStringLiteral("programLive")).toMap().value(QStringLiteral("permissions")).toStringList(),
            (QStringList { QStringLiteral("ask"), QStringLiteral("auto") }));
        QVERIFY(f.workspace.sessionActions().changeAgent(test::id(1), { { QStringLiteral("permissions"), QStringLiteral("ask") } }));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.agent"));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("change")).toObject(),
            QJsonObject({ { QStringLiteral("permissions"), QStringLiteral("ask") } }));
        QVERIFY(!f.workspace.sessionActions().changeAgent(test::id(1), {}));
        auto invalid = test::session(2);
        invalid.insert(QStringLiteral("programSettings"), QJsonObject { { QStringLiteral("permissions"), QStringLiteral("skip") } });
        f.workspace.apply(test::snapshot({ invalid }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void aTerminalShowsTheStateThatTheRuntimeGives()
    {
        Fixture f;
        const auto shown = [](const int number, const QJsonObject& status, const bool working, const QString& waiting = {},
                               const QString& activity = {}) {
            auto session = test::session(number);
            session.insert(QStringLiteral("programStatus"), status);
            session.insert(QStringLiteral("working"), working);
            if (!waiting.isEmpty())
                session.insert(QStringLiteral("waiting"), waiting);
            if (!activity.isEmpty())
                session.insert(QStringLiteral("activity"), activity);
            return session;
        };
        const auto field = [&](const int number, const QString& name) {
            return f.sessions.presentationForSession(test::id(number)).value(name);
        };
        const auto activity = [&](const int number) { return field(number, QStringLiteral("activity")).toString(); };
        const auto sign = [&](const int number) { return field(number, QStringLiteral("sign")).toString(); };
        const QString blocked = QStringLiteral("blocked");
        f.workspace.apply(test::snapshot({
            shown(1, {{QStringLiteral("state"), QStringLiteral("working")}, {QStringLiteral("progress"), 40}}, true, {}, QStringLiteral("Build")),
            shown(2, {{QStringLiteral("state"), QStringLiteral("idle")}}, false, {}, QStringLiteral("Build")),
        }), 0);
        QVERIFY(field(1, QStringLiteral("working")).toBool());
        QVERIFY(!field(2, QStringLiteral("working")).toBool());
        QCOMPARE(activity(2), QStringLiteral("Build"));
        QCOMPARE(field(1, QStringLiteral("progress")).toInt(), 40);
        QCOMPARE(field(2, QStringLiteral("progress")).toInt(), -1);
        QVERIFY(sign(1).isEmpty());

        f.workspace.apply(test::snapshot({
            shown(1, {{QStringLiteral("state"), blocked}, {QStringLiteral("kind"), QStringLiteral("permission")},
                {QStringLiteral("title"), QStringLiteral("Review")}, {QStringLiteral("message"), QStringLiteral("Allow the command?")}},
                false, blocked, QStringLiteral("Review: Allow the command?")),
            shown(2, {{QStringLiteral("state"), blocked}, {QStringLiteral("kind"), QStringLiteral("question")}}, false, blocked),
        }), 0);
        QVERIFY(!field(1, QStringLiteral("working")).toBool());
        QCOMPARE(activity(1), QStringLiteral("Review: Allow the command?"));
        QVERIFY(activity(2).isEmpty());
        QCOMPARE(sign(1), QStringLiteral("hand"));
        QCOMPARE(sign(2), QStringLiteral("question"));
        QVERIFY(field(1, QStringLiteral("asks")).toBool());

        f.workspace.apply(test::snapshot({
            shown(1, {{QStringLiteral("state"), blocked}, {QStringLiteral("kind"), QStringLiteral("permission")}}, false, blocked),
            shown(2, {{QStringLiteral("state"), QStringLiteral("error")}}, false, QStringLiteral("error")),
        }), 0);
        QVERIFY(activity(2).isEmpty());
        QCOMPARE(sign(2), QStringLiteral("failed"));
        QVERIFY(!field(2, QStringLiteral("asks")).toBool());

        f.workspace.apply(test::snapshot({
            shown(1, {{QStringLiteral("state"), blocked}, {QStringLiteral("kind"), QStringLiteral("auth")}}, false, blocked),
            shown(2, {{QStringLiteral("state"), QStringLiteral("done")}}, false, QStringLiteral("done")),
        }), 0);
        QCOMPARE(sign(1), QStringLiteral("key"));
        QCOMPARE(sign(2), QStringLiteral("done"));
        f.workspace.apply(test::snapshot({
            shown(1, {{QStringLiteral("state"), QStringLiteral("working")}, {QStringLiteral("progress"), 101}}, true),
        }), 0);
        QCOMPARE(sign(2), QStringLiteral("done"));

        f.workspace.apply(test::snapshot({ shown(1, {{QStringLiteral("state"), QStringLiteral("dance")}}, false) }), 0);
        QCOMPARE(f.sessions.session(test::id(2))->waiting, QStringLiteral("done"));
        f.workspace.apply(test::snapshot({ shown(1, {{QStringLiteral("state"), QStringLiteral("done")}}, false, QStringLiteral("dance")) }), 0);
        QCOMPARE(f.sessions.session(test::id(2))->waiting, QStringLiteral("done"));
    }
    void aTerminalNamesWhereItsProgramRunsAndWhetherAsRoot()
    {
        Fixture f;
        const auto place = [](const QString& kind, const QString& name) {
            return QJsonObject { { QStringLiteral("kind"), kind }, { QStringLiteral("name"), name } };
        };
        const auto placed = [&](const int number, const QJsonValue& where, const bool root) {
            auto session = test::session(number);
            QJsonObject context { { QStringLiteral("root"), root } };
            if (!where.isUndefined())
                context.insert(QStringLiteral("place"), where);
            session.insert(QStringLiteral("context"), context);
            return session;
        };
        const auto field = [&](const int number, const QString& name) {
            return f.sessions.presentationForSession(test::id(number)).value(name);
        };
        f.workspace.apply(test::snapshot({
            placed(1, place(QStringLiteral("remote"), QStringLiteral("build-box")), false),
            placed(2, place(QStringLiteral("container"), QStringLiteral("web")), true),
            placed(3, QJsonValue(QJsonValue::Undefined), true),
            test::session(4),
        }), 0);
        QCOMPARE(field(1, QStringLiteral("placeKind")).toString(), QStringLiteral("remote"));
        QCOMPARE(field(1, QStringLiteral("placeName")).toString(), QStringLiteral("build-box"));
        QCOMPARE(field(1, QStringLiteral("placeLabel")).toString(), QStringLiteral("On build-box"));
        QVERIFY(!field(1, QStringLiteral("root")).toBool());
        QCOMPARE(field(2, QStringLiteral("placeLabel")).toString(), QStringLiteral("In the container web"));
        QVERIFY(field(2, QStringLiteral("root")).toBool());
        QVERIFY(field(3, QStringLiteral("placeKind")).toString().isEmpty());
        QVERIFY(field(3, QStringLiteral("root")).toBool());
        QVERIFY(field(4, QStringLiteral("placeLabel")).toString().isEmpty());
        QVERIFY(!field(4, QStringLiteral("root")).toBool());

        f.workspace.apply(test::snapshot({
            placed(1, place(QStringLiteral("moon"), QStringLiteral("base")), false),
        }), 0);
        QCOMPARE(field(1, QStringLiteral("placeKind")).toString(), QStringLiteral("remote"));
    }
    void aProjectTerminalThatIsNotOpenHereShowsTheStatusOfItsHost()
    {
        Fixture f;
        const auto closed = [](const int number) {
            auto session = test::session(number, true, false);
            session.insert(QStringLiteral("connectionState"), QStringLiteral("offline"));
            return session;
        };
        auto asking = closed(1);
        asking.insert(QStringLiteral("programStatus"),
            QJsonObject { { QStringLiteral("state"), QStringLiteral("blocked") }, { QStringLiteral("kind"), QStringLiteral("question") } });
        asking.insert(QStringLiteral("waiting"), QStringLiteral("blocked"));
        auto working = closed(2);
        working.insert(QStringLiteral("programStatus"), QJsonObject { { QStringLiteral("state"), QStringLiteral("working") } });
        working.insert(QStringLiteral("working"), true);
        f.workspace.apply(test::snapshot({ asking, working, closed(3) }), 0);
        const auto field = [&](const int number, const QString& name) {
            return f.sessions.presentationForSession(test::id(number)).value(name);
        };
        QCOMPARE(field(1, QStringLiteral("sign")).toString(), QStringLiteral("question"));
        QVERIFY(field(2, QStringLiteral("working")).toBool());
        QVERIFY(!field(3, QStringLiteral("working")).toBool());
    }
    void minimizeKeepsProcessAndClosePrunesFromCatalog()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        f.commands.values.clear();
        QVERIFY(f.workspace.sessionActions().minimize(test::id(1)));
        QVERIFY(f.commands.values.isEmpty());
        QVERIFY(f.sessions.containsSession(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().close(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.close"));
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(f.sessions.containsSession(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(1)));
        const auto request = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(!f.sessions.containsSession(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("session.result") }, { QStringLiteral("requestId"), request },
            { QStringLiteral("operation"), QStringLiteral("session.close") } }, 0);
        QVERIFY(!f.workspace.sessionActions().busy());
    }
    void closingStatusDisablesActivationAndControl()
    {
        Fixture f;
        auto closing = test::session(1);
        closing.insert(QStringLiteral("status"), QStringLiteral("closing"));
        f.workspace.apply(test::snapshot({ closing }), 0);
        QVERIFY(f.sessions.hasAuthoritativeSnapshot());
        QCOMPARE(f.sessions.session(test::id(1))->status, QStringLiteral("closing"));
        QVERIFY(!f.sessions.presentationSession(test::id(1))->canControl);
        QVERIFY(!f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.commands.values.isEmpty());
        closing.insert(QStringLiteral("status"), QStringLiteral("stopping"));
        f.workspace.apply(test::snapshot({ closing }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void aClosingTerminalLeavesTheStageAndTheSidebarAtOnce()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(1)));
        auto closing = test::session(1);
        closing.insert(QStringLiteral("status"), QStringLiteral("closing"));
        f.workspace.apply(test::snapshot({ closing }), 0);
        QVERIFY(f.sessions.containsSession(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        QVERIFY(f.sessions.listedSessions().isEmpty());
        QVERIFY(f.sessions.folderGroups().isEmpty());
    }
    void aClosingTerminalLeavesItsProjectAtOnce()
    {
        Fixture f;
        auto session = test::session(1);
        session.insert(QStringLiteral("projectId"), test::id(10));
        f.workspace.apply(test::snapshot({ session }), 0);
        f.workspace.projects().open(test::id(10));
        QCOMPARE(f.workspace.projects().sessionIds(), QStringList { test::id(1) });
        session.insert(QStringLiteral("status"), QStringLiteral("closing"));
        f.workspace.apply(test::snapshot({ session }), 0);
        QVERIFY(f.workspace.projects().sessionIds().isEmpty());
    }
    void aTerminalThatWaitsForItsProgramsToStopDoesNotHoldUpANewOne()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.workspace.sessionActions().close(test::id(1)));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.close"));
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(f.workspace.sessionActions().create(QStringLiteral("Work"), QStringLiteral("/repo")));
        QVERIFY(f.workspace.sessionActions().busy());
    }
    void aCreatedTerminalOpensWhenItsResultArrives()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.sessionActions().create(QStringLiteral("Work"), QStringLiteral("/repo")));
        const auto request = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.desktop.selectedSessionId().isEmpty());
        QVERIFY(f.workspace.sessionActions().busy());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("session.result") }, { QStringLiteral("requestId"), request },
            { QStringLiteral("operation"), QStringLiteral("session.create") }, { QStringLiteral("sessionId"), test::id(1) } }, 0);
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));
        QVERIFY(!f.workspace.sessionActions().busy());
    }
    void aQuickTerminalStartsPlainInTheFolderOfTheSelectedTerminalOfThisComputer()
    {
        Fixture f;
        auto selected = test::session(1);
        selected.insert(QStringLiteral("workingDir"), f.dir.path());
        f.workspace.apply(test::snapshot({ selected }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().createQuick());
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("session.create"));
        QCOMPARE(sent.value(QStringLiteral("workingDir")).toString(), f.dir.path());
        QVERIFY(!sent.contains(QStringLiteral("agent")));
        QVERIFY(!sent.contains(QStringLiteral("command")));
        QVERIFY(!sent.contains(QStringLiteral("computer")));
    }
    void sameAgentStartsTheAgentOfTheSelectedTerminalWithItsAccountAndChoices()
    {
        Fixture f;
        f.workspace.sessionActions().setAccounts([](const QString& agent) {
            return agent == QStringLiteral("claude") ? QStringList { QStringLiteral("claude"), QStringLiteral("work") }
                                                     : QStringList {};
        });
        auto claude = test::session(1);
        claude.insert(QStringLiteral("workingDir"), f.dir.path());
        claude.insert(QStringLiteral("program"), QStringLiteral("claude"));
        claude.insert(QStringLiteral("programAccount"), 1);
        claude.insert(QStringLiteral("programSettings"),
            QJsonObject { { QStringLiteral("model"), QStringLiteral("opus") }, { QStringLiteral("effort"), QStringLiteral("high") },
                { QStringLiteral("permissions"), QStringLiteral("auto") }, { QStringLiteral("output"), QStringLiteral("detailed") } });
        claude.insert(QStringLiteral("programPending"), QJsonObject { { QStringLiteral("model"), QStringLiteral("sonnet") } });
        f.workspace.apply(test::snapshot({ claude, test::session(2) }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().createSameAgent());
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("workingDir")).toString(), f.dir.path());
        QCOMPARE(sent.value(QStringLiteral("agent")).toObject(),
            (QJsonObject { { QStringLiteral("agent"), QStringLiteral("claude") }, { QStringLiteral("account"), QStringLiteral("work") },
                { QStringLiteral("model"), QStringLiteral("sonnet") }, { QStringLiteral("effort"), QString() },
                { QStringLiteral("permissions"), QStringLiteral("auto") }, { QStringLiteral("output"), QStringLiteral("detailed") } }));

        f.workspace.apply(test::snapshot({ claude, test::session(2), test::session(3) }), 0);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("session.result") },
                              { QStringLiteral("requestId"), sent.value(QStringLiteral("requestId")) },
                              { QStringLiteral("operation"), QStringLiteral("session.create") },
                              { QStringLiteral("sessionId"), test::id(3) } },
            0);
        f.workspace.sessionActions().giveTypedKeys(test::id(3));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.workspace.sessionActions().createSameAgent());
        QVERIFY(!f.commands.values.last().contains(QStringLiteral("agent")));
    }
    void aTerminalOfThisComputerAsksForTheGridOfItsTile()
    {
        Fixture f;
        const QJsonObject size { { QStringLiteral("cols"), 70 }, { QStringLiteral("rows"), 17 } };
        f.workspace.sessionActions().setStartSize([size] { return size; });
        QVERIFY(f.workspace.sessionActions().create(QStringLiteral("Here"), f.dir.path()));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("size")).toObject(), size);
        QVERIFY(f.workspace.sessionActions().create(
            QStringLiteral("There"), f.dir.path(), {}, {}, {}, {}, QStringLiteral("device-2")));
        QVERIFY(!f.commands.values.last().contains(QStringLiteral("size")));
    }
    void aQuickTerminalStartsInTheLauncherFolderWithoutAFolderOfThisComputer_data()
    {
        QTest::addColumn<int>("selected");
        QTest::newRow("no selection") << 0;
        QTest::newRow("a terminal of another computer") << 1;
        QTest::newRow("a folder that is gone") << 2;
    }
    void aQuickTerminalStartsInTheLauncherFolderWithoutAFolderOfThisComputer()
    {
        QFETCH(int, selected);
        Fixture f;
        f.settings.setWorkingDirectory(f.dir.path());
        auto gone = test::session(2);
        gone.insert(QStringLiteral("workingDir"), f.dir.path() + QStringLiteral("/gone"));
        f.workspace.apply(test::snapshot({ test::session(1, true), gone }), 0);
        if (selected != 0)
            QVERIFY(f.workspace.sessionActions().activate(test::id(selected)));
        QVERIFY(f.workspace.sessionActions().createQuick());
        QCOMPARE(f.commands.values.last().value(QStringLiteral("workingDir")).toString(),
            f.settings.effectiveWorkingDirectory());
    }
    void aSecondQuickTerminalWaitsForTheFirst()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.sessionActions().createQuick());
        const auto request = f.commands.values.last().value(QStringLiteral("requestId"));
        QVERIFY(!f.workspace.sessionActions().createQuick());
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("session.result") }, { QStringLiteral("requestId"), request },
            { QStringLiteral("operation"), QStringLiteral("session.create") }, { QStringLiteral("sessionId"), test::id(1) } }, 0);
        f.workspace.sessionActions().giveTypedKeys(test::id(1));
        QVERIFY(f.workspace.sessionActions().createQuick());
    }
    void aQuickTerminalSaysSoWhenTheStageIsFullAndStartsNothing()
    {
        Fixture f;
        QJsonArray sessions;
        for (int value = 1; value <= 6; ++value)
            sessions.append(test::session(value));
        f.workspace.apply(test::snapshot(sessions), 0);
        for (int value = 1; value <= 6; ++value)
            QVERIFY(f.workspace.sessionActions().activate(test::id(value)));
        const auto before = f.commands.values.size();
        QVERIFY(!f.workspace.sessionActions().createQuick());
        QCOMPARE(f.commands.values.size(), before);
        QVERIFY(!f.workspace.error().isEmpty());
    }
    void aFullStageInFocusTakesOffTheTerminalLookedAtLongestAgo()
    {
        Fixture f;
        QJsonArray sessions;
        for (int value = 1; value <= 7; ++value)
            sessions.append(test::session(value));
        f.workspace.apply(test::snapshot(sessions), 0);
        for (int value = 1; value <= 6; ++value)
            QVERIFY(f.workspace.sessionActions().activate(test::id(value)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(!f.workspace.sessionActions().activate(test::id(7)));
        f.desktop.toggleFocusForSelectedSession();
        QVERIFY(f.workspace.sessionActions().activate(test::id(7)));
        QCOMPARE(f.desktop.stagedSessionIds().size(), 6);
        QVERIFY(!f.desktop.stagedSessionIds().contains(test::id(2)));
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(1)));
        QCOMPARE(f.desktop.selectedSessionId(), test::id(7));
        QCOMPARE(f.desktop.stageLayoutMode(), kodosi::DesktopStateModel::Focus);
        QVERIFY(f.sessions.containsSession(test::id(2)));
    }
    void aRemoteTerminalThatCannotOpenLeavesAFullStageAsItWas()
    {
        Fixture f;
        QJsonArray sessions;
        for (int value = 1; value <= 6; ++value)
            sessions.append(test::session(value));
        sessions.append(test::session(7, true));
        f.workspace.apply(test::snapshot(sessions), 0);
        for (int value = 1; value <= 6; ++value)
            QVERIFY(f.workspace.sessionActions().activate(test::id(value)));
        f.desktop.toggleFocusForSelectedSession();
        const auto staged = f.desktop.stagedSessionIds();
        f.commands.reject = true;
        QVERIFY(!f.workspace.sessionActions().activate(test::id(7)));
        QCOMPARE(f.desktop.stagedSessionIds(), staged);
    }
    void aLinkedTerminalThatCannotComeOnAFullStageIsDisconnected()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        QJsonArray sessions;
        for (int value = 1; value <= 6; ++value)
            sessions.append(test::session(value));
        f.workspace.apply(test::snapshot(sessions), 1);
        for (int value = 1; value <= 6; ++value)
            QVERIFY(f.workspace.sessionActions().activate(test::id(value)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(7)));
        QVERIFY(opened(f.commands, test::id(7)));
        sessions.append(test::session(7, true));
        f.workspace.apply(test::snapshot(sessions), 1);
        QVERIFY(!f.desktop.stagedSessionIds().contains(test::id(7)));
        const auto last = f.commands.values.last();
        QCOMPARE(last.value(QStringLiteral("type")).toString(), QStringLiteral("session.disconnect"));
        QCOMPARE(last.value(QStringLiteral("sessionId")).toString(), test::id(7));
    }
    void aTerminalStartedForARoomJoinsThatProject()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({}), 0);
        const QVariantMap agent { { QStringLiteral("agent"), QStringLiteral("claude") },
            { QStringLiteral("account"), QStringLiteral("work") }, { QStringLiteral("model"), QString() },
            { QStringLiteral("effort"), QStringLiteral("high") } };
        QVERIFY(f.workspace.sessionActions().create(
            {}, QStringLiteral("/repo"), {}, QStringLiteral(" fix the test "), test::id(7), agent));
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("session.create"));
        QCOMPARE(sent.value(QStringLiteral("projectId")).toString(), test::id(7));
        QVERIFY(!sent.contains(QStringLiteral("command")));
        QCOMPARE(sent.value(QStringLiteral("agent")).toObject(), QJsonObject::fromVariantMap(agent));
        QCOMPARE(sent.value(QStringLiteral("prompt")).toString(), QStringLiteral("fix the test"));
        QVERIFY(!sent.value(QStringLiteral("name")).toString().isEmpty());
    }
    void aTerminalOnANewBranchSendsTheBranchAndTheChosenFolder()
    {
        Fixture f;
        auto inRepository = test::session(1);
        inRepository.insert(QStringLiteral("repository"), true);
        inRepository.insert(QStringLiteral("workingDir"), QStringLiteral("/repo"));
        auto elsewhere = test::session(2);
        elsewhere.insert(QStringLiteral("workingDir"), QStringLiteral("/notes"));
        f.workspace.apply(test::snapshot({ inRepository, elsewhere }), 0);
        const auto groups = f.sessions.folderGroups();
        QCOMPARE(groups.size(), 2);
        QVERIFY(!groups.at(0).toMap().value(QStringLiteral("repository")).toBool());
        QVERIFY(groups.at(1).toMap().value(QStringLiteral("repository")).toBool());

        auto& actions = f.workspace.sessionActions();
        QVERIFY(!actions.createOnBranch(QStringLiteral("/repo"), QStringLiteral("  ")));
        QVERIFY(actions.createOnBranch(QStringLiteral("/repo"), QStringLiteral(" fix/login ")));
        auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("session.create"));
        QCOMPARE(sent.value(QStringLiteral("workingDir")).toString(), QStringLiteral("/repo"));
        QCOMPARE(sent.value(QStringLiteral("branch")).toString(), QStringLiteral("fix/login"));
        QCOMPARE(sent.value(QStringLiteral("name")).toString(), QStringLiteral("fix/login"));
        QVERIFY(!sent.contains(QStringLiteral("worktrees")));

        f.settings.setBranchFolder(f.dir.path());
        QVERIFY(actions.createOnBranch(QStringLiteral("/repo"), QStringLiteral("fix/logout")));
        sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("worktrees")).toString(), QFileInfo(f.dir.path()).canonicalFilePath());

        QVERIFY(actions.create(QStringLiteral("Work"), QStringLiteral("/repo")));
        QVERIFY(!f.commands.values.last().contains(QStringLiteral("branch")));
        QVERIFY(!f.commands.values.last().contains(QStringLiteral("worktrees")));
        const auto suggested = actions.suggestedBranch();
        QVERIFY(!suggested.isEmpty() && !suggested.contains(QLatin1Char(' ')) && suggested == suggested.toLower());
    }
    void startupLinkSurvivesEmptyCatalogAndSignIn()
    {
        Fixture f;
        f.workspace.route({ test::id(1) });
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.finalizing") } }, 0);
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.commands.values.isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("accountEpoch"), 0 } }, 0);
        f.workspace.apply(test::snapshot({}), 0);
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Settings);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("session.openRemote"));
        f.workspace.apply(test::snapshot({ test::session(1, true, false) }), 0);
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));
    }
    void queuedStartupLinksAreNotOverwritten()
    {
        Fixture f;
        f.workspace.route({ test::id(1) });
        f.workspace.route({ test::id(2) });
        f.workspace.apply(test::snapshot({ test::session(1, true, false), test::session(2, true, false) }), 0);
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(2)));
    }
    void accountChangeClearsViewsAndPending()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().close(test::id(1)));
        f.desktop.setLauncherPrompt(QStringLiteral("fix the build"));
        f.desktop.setLauncherFolder(QStringLiteral("/work/atlas"));
        f.desktop.setLauncherRemoteFolder(QStringLiteral("studio"), QStringLiteral("/home/me/atlas"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("accountEpoch"), 2 } }, 2);
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        QVERIFY(f.desktop.launcherPrompt().isEmpty() && f.desktop.launcherFolder().isEmpty());
        QVERIFY(f.desktop.launcherRemoteFolders().isEmpty());
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(!f.workspace.account().signedIn());
    }
    void theStageComesBackForItsAccountAfterARestart()
    {
        QTemporaryDir dir;
        const auto both = test::snapshot({ test::session(1), test::session(2) });
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent(test::id(50), 1), 1);
            first.workspace.apply(both, 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(2)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent(test::id(50), 1), 1);
        restarted.workspace.apply(test::snapshot({}), 1);
        QVERIFY(restarted.desktop.stagedSessionIds().isEmpty());
        restarted.workspace.apply(both, 1);
        QCOMPARE(restarted.desktop.stagedSessionIds(), QStringList { test::id(2) });
        QCOMPARE(restarted.desktop.selectedSessionId(), test::id(2));

        restarted.workspace.apply(accountEvent({}, 2), 2);
        restarted.workspace.apply(both, 2);
        QVERIFY(restarted.desktop.stagedSessionIds().isEmpty());
        QVERIFY(restarted.workspace.sessionActions().activate(test::id(1)));
        restarted.workspace.apply(accountEvent(test::id(50), 3), 3);
        restarted.workspace.apply(both, 3);
        QCOMPARE(restarted.desktop.stagedSessionIds(), QStringList { test::id(2) });

        Fixture signedOut(dir.path());
        signedOut.workspace.apply(accountEvent({}, 1), 1);
        signedOut.workspace.apply(both, 1);
        QCOMPARE(signedOut.desktop.stagedSessionIds(), QStringList { test::id(1) });
        QVERIFY(signedOut.workspace.sessionActions().close(test::id(1)));
        signedOut.workspace.apply(test::snapshot({ test::session(2) }), 1);
        Fixture again(dir.path());
        again.workspace.apply(accountEvent({}, 1), 1);
        again.workspace.apply(both, 1);
        QVERIFY(again.desktop.stagedSessionIds().isEmpty());
    }
    void aTerminalThatTheHostStartedAgainTakesThePlaceOfTheOneBefore()
    {
        QTemporaryDir dir;
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent({}, 1), 1);
            first.workspace.apply(test::snapshot({ test::session(1) }), 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(1)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent({}, 1), 1);
        restarted.workspace.apply(accountEvent(test::id(50), 2), 2);
        const auto restores = std::ranges::count_if(restarted.commands.values, [](const QJsonObject& command) {
            return command.value(QStringLiteral("type")).toString() == QStringLiteral("session.restore");
        });
        QCOMPARE(restores, 1);
        restarted.workspace.apply(accountEvent({}, 3), 3);
        auto again = test::session(2);
        again.insert(QStringLiteral("restoredFrom"), test::id(1));
        restarted.workspace.apply(test::snapshot({ again }), 3);
        QCOMPARE(restarted.desktop.stagedSessionIds(), QStringList { test::id(2) });
    }
    void restoredTerminalsKeepTheirOrderOnTheStage()
    {
        QTemporaryDir dir;
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent({}, 1), 1);
            first.workspace.apply(test::snapshot({ test::session(1), test::session(2) }), 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(1)));
            QVERIFY(first.workspace.sessionActions().activate(test::id(2)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent({}, 1), 1);
        auto second = test::session(4);
        second.insert(QStringLiteral("restoredFrom"), test::id(2));
        restarted.workspace.apply(test::snapshot({ second }), 1);
        auto first = test::session(3);
        first.insert(QStringLiteral("restoredFrom"), test::id(1));
        restarted.workspace.apply(test::snapshot({ first, second }), 1);
        QCOMPARE(restarted.desktop.stagedSessionIds(), (QStringList { test::id(3), test::id(4) }));
    }
    void theStageIsWrittenOnlyWhenItChanges()
    {
        QTemporaryDir dir;
        const auto file = dir.filePath(QStringLiteral("ui.ini"));
        Fixture f(dir.path());
        f.workspace.apply(accountEvent({}, 1), 1);
        f.workspace.apply(test::snapshot({ test::session(1), test::session(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QTRY_VERIFY(QFileInfo::exists(file));
        QVERIFY(QFile::remove(file));
        auto renamed = test::session(2);
        renamed.insert(QStringLiteral("name"), QStringLiteral("Build"));
        f.workspace.apply(test::snapshot({ test::session(1), renamed }), 1);
        f.workspace.apply(test::snapshot({ test::session(1), test::session(2) }), 1);
        QCoreApplication::processEvents();
        QVERIFY(!QFileInfo::exists(file));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QTRY_VERIFY(QFileInfo::exists(file));
    }
    void theStageComesBackWhileASavedSignInWaitsForTheServer()
    {
        QTemporaryDir dir;
        const QJsonObject restoring { { QStringLiteral("type"), QStringLiteral("auth.restoring") },
            { QStringLiteral("reason"), QStringLiteral("offline") }, { QStringLiteral("accountEpoch"), 1 } };
        {
            Fixture first(dir.path());
            first.workspace.apply(restoring, 1);
            first.workspace.apply(test::snapshot({ test::session(1) }), 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(1)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(restoring, 1);
        auto again = test::session(2);
        again.insert(QStringLiteral("restoredFrom"), test::id(1));
        restarted.workspace.apply(test::snapshot({ again }), 1);
        QCOMPARE(restarted.desktop.stagedSessionIds(), QStringList { test::id(2) });
    }
    void aRestoredRemoteTerminalOpensAgain()
    {
        QTemporaryDir dir;
        const auto remote = test::snapshot({ test::session(4, true, false) });
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent(test::id(50), 1), 1);
            first.workspace.apply(remote, 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(4)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent(test::id(50), 1), 1);
        restarted.workspace.apply(remote, 1);
        QCOMPARE(restarted.desktop.stagedSessionIds(), QStringList { test::id(4) });
        const auto command = restarted.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("session.openRemote"));
        QCOMPARE(command.value(QStringLiteral("sessionId")).toString(), test::id(4));
    }
    void anOpenOfAListedTerminalFailsWithoutAnErrorAndOnlyALinkToAnUnknownTerminalShowsOne()
    {
        QTemporaryDir dir;
        const auto remote = test::snapshot({ test::session(4, true, false) });
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent(test::id(50), 1), 1);
            first.workspace.apply(remote, 1);
            QVERIFY(first.workspace.sessionActions().activate(test::id(4)));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent(test::id(50), 1), 1);
        restarted.workspace.apply(remote, 1);
        restarted.workspace.apply(openFailure(lastOpen(restarted.commands)), 1);
        QVERIFY(restarted.workspace.error().isEmpty());
        QVERIFY(restarted.workspace.sessionActions().activate(test::id(4)));
        restarted.workspace.apply(openFailure(lastOpen(restarted.commands)), 1);
        QVERIFY(restarted.workspace.error().isEmpty());
        QVERIFY(restarted.workspace.sessionActions().activate(test::id(9)));
        const auto link = lastOpen(restarted.commands);
        QCOMPARE(link.value(QStringLiteral("sessionId")).toString(), test::id(9));
        restarted.workspace.apply(openFailure(link), 1);
        QCOMPARE(restarted.workspace.error(), QStringLiteral("Kodosi could not reach the terminal connection service."));
    }
    void aTerminalThatAnotherComputerStartedAgainTakesThePlaceAndFocusOfTheOneThatEnded()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.desktop.selectSession(test::id(1)));
        f.desktop.toggleFocusForSelectedSession();
        f.workspace.projects().open(test::id(10));
        f.workspace.projects().setPresentation(QStringLiteral("terminal"), test::id(1));
        f.workspace.apply(test::snapshot({ onMini(2) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), QStringList { test::id(2) });
        QVERIFY(f.desktop.selectedSessionId().isEmpty());
        QCOMPARE(f.desktop.stageLayoutMode(), kodosi::DesktopStateModel::Grid);
        f.workspace.apply(test::snapshot({ onMini(2), onMini(3, 1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), (QStringList { test::id(3), test::id(2) }));
        QCOMPARE(f.desktop.selectedSessionId(), test::id(3));
        QCOMPARE(f.desktop.stageLayoutMode(), kodosi::DesktopStateModel::Focus);
        QVERIFY(opened(f.commands, test::id(3)));
        QCOMPARE(f.workspace.projects().presentation().value(QStringLiteral("terminal")).toString(), test::id(3));
    }
    void aRestartTakesNoSelectionThatThePersonGaveToAnotherTerminal()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.desktop.selectSession(test::id(1)));
        f.workspace.apply(test::snapshot({ onMini(2) }), 1);
        QVERIFY(f.desktop.selectSession(test::id(2)));
        f.workspace.apply(test::snapshot({ onMini(2), onMini(3, 1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), (QStringList { test::id(3), test::id(2) }));
        QCOMPARE(f.desktop.selectedSessionId(), test::id(2));
    }
    void terminalsThatEndedTogetherComeBackInTheirPlacesAndOnlyTheSelectedOneTakesTheSelection()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        f.workspace.apply(test::snapshot({}), 1);
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        f.workspace.apply(test::snapshot({ onMini(4, 2), onMini(3, 1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), (QStringList { test::id(3), test::id(4) }));
        QCOMPARE(f.desktop.selectedSessionId(), test::id(4));
        QVERIFY(opened(f.commands, test::id(3)) && opened(f.commands, test::id(4)));
    }
    void terminalsThatEndedOneAfterAnotherComeBackInTheirPlaces()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2), onMini(3) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(3)));
        f.workspace.apply(test::snapshot({ onMini(2), onMini(3) }), 1);
        f.workspace.apply(test::snapshot({ onMini(2) }), 1);
        f.workspace.apply(test::snapshot({}), 1);
        f.workspace.apply(test::snapshot({ onMini(6, 3), onMini(5, 2), onMini(4, 1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), (QStringList { test::id(4), test::id(5), test::id(6) }));
    }
    void aTerminalThatIsStillListedMovesToItsRestartAtOnce()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2, 1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), QStringList { test::id(2) });
        QCOMPARE(f.desktop.selectedSessionId(), test::id(2));
    }
    void aTerminalFollowsNoRestartOnAnotherComputerOfAnotherPersonOrAfterAClose()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.workspace.sessionActions().close(test::id(2)));
        f.workspace.apply(test::snapshot({}), 1);
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        QJsonArray restarts { onMini(3, 1, test::id(50), QStringLiteral("air")), onMini(4, 1, test::id(51)), onMini(5, 2) };
        f.workspace.apply(test::snapshot(restarts), 1);
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        restarts.append(onMini(6, 1));
        f.workspace.apply(test::snapshot(restarts), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), QStringList { test::id(6) });
    }
    void aPinStaysWithItsAccountAndFollowsItsTerminal()
    {
        QTemporaryDir dir;
        {
            Fixture first(dir.path());
            first.workspace.apply(accountEvent(test::id(50), 1), 1);
            first.workspace.apply(test::snapshot({ test::session(1), onMini(2) }), 1);
            first.desktop.togglePin(test::id(2));
            first.desktop.togglePin(test::id(1));
            QCOMPARE(first.desktop.pinnedSessionIds(), (QStringList { test::id(2), test::id(1) }));
            first.workspace.apply(test::snapshot({ test::session(1) }), 1);
            first.workspace.apply(test::snapshot({ test::session(1), onMini(3, 2) }), 1);
            QCOMPARE(first.desktop.pinnedSessionIds(), (QStringList { test::id(3), test::id(1) }));
        }
        Fixture restarted(dir.path());
        restarted.workspace.apply(accountEvent(test::id(50), 1), 1);
        auto local = test::session(4);
        local.insert(QStringLiteral("restoredFrom"), test::id(1));
        restarted.workspace.apply(test::snapshot({ local, onMini(3, 2) }), 1);
        QCOMPARE(restarted.desktop.pinnedSessionIds(), (QStringList { test::id(3), test::id(4) }));
        restarted.desktop.togglePin(test::id(3));
        QCOMPARE(restarted.desktop.pinnedSessionIds(), QStringList { test::id(4) });
        restarted.workspace.apply(accountEvent(test::id(51), 2), 2);
        QVERIFY(restarted.desktop.pinnedSessionIds().isEmpty());
    }
    void aDraggedTerminalTakesThePlaceOfTheOneItDropsOn()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ test::session(1), test::session(2), test::session(3) }), 1);
        for (const auto id : { 1, 2, 3 })
            QVERIFY(f.workspace.sessionActions().activate(test::id(id)));
        QVERIFY(f.desktop.moveStaged(test::id(1), test::id(3)));
        QCOMPARE(f.desktop.stagedSessionIds(), (QStringList { test::id(2), test::id(3), test::id(1) }));
        QVERIFY(!f.desktop.moveStaged(test::id(9), test::id(2)));
    }
    void closingTheSelectedTerminalSelectsTheFirstTerminalLeft()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        f.workspace.apply(test::snapshot({ onMini(1), onMini(2) }), 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        QVERIFY(f.workspace.sessionActions().activate(test::id(2)));
        QVERIFY(f.workspace.sessionActions().close(test::id(2)));
        f.workspace.apply(test::snapshot({ onMini(1) }), 1);
        QCOMPARE(f.desktop.stagedSessionIds(), QStringList { test::id(1) });
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));
    }
    void boundedStageAndInvalidSnapshot()
    {
        Fixture f;
        QJsonArray entries;
        for (int i = 1; i <= 7; ++i)
            entries.append(test::session(i));
        f.workspace.apply(test::snapshot(entries), 0);
        for (int i = 1; i <= 6; ++i)
            QVERIFY(f.workspace.sessionActions().activate(test::id(i)));
        QVERIFY(!f.workspace.sessionActions().activate(test::id(7)));
        auto invalid = test::session(9);
        invalid.insert(QStringLiteral("incarnationId"), QStringLiteral("bad"));
        f.workspace.apply(test::snapshot({ invalid }), 0);
        QCOMPARE(f.sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
    }
    void projectRepliesAreRequestScopedAndDeleteKeepsTerminal()
    {
        Fixture f;
        auto terminal = test::session(1);
        terminal.insert(QStringLiteral("projectId"), test::id(11));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        f.workspace.projects().open(test::id(10));
        const auto oldRequest = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.projects().open(test::id(11));
        const auto request = f.commands.values.last().value(QStringLiteral("requestId"));
        const QJsonObject project { { QStringLiteral("id"), test::id(11) },
            { QStringLiteral("name"), QStringLiteral("Project") },
            { QStringLiteral("ownerUserId"), test::id(50) } };
        QJsonObject reply { { QStringLiteral("type"), QStringLiteral("project.result") },
            { QStringLiteral("operation"), QStringLiteral("project.open") },
            { QStringLiteral("requestId"), oldRequest }, { QStringLiteral("project"), project },
            { QStringLiteral("members"), QJsonArray {} } };
        f.workspace.apply(reply, 0);
        QVERIFY(f.workspace.projects().selectedProject().isEmpty());
        reply.insert(QStringLiteral("requestId"), request);
        f.workspace.apply(reply, 0);
        QCOMPARE(f.workspace.projects().sessionIds(), QStringList { test::id(1) });
        f.workspace.apply({{QStringLiteral("type"),QStringLiteral("projects.snapshot")},{QStringLiteral("projects"),QJsonArray{}},{QStringLiteral("invitations"),QJsonArray{}}}, 0);
        QVERIFY(f.workspace.projects().selectedProjectId().isEmpty());
        f.workspace.projects().open(test::id(11));
        f.workspace.projects().remove();
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.result") },
            { QStringLiteral("operation"), QStringLiteral("project.delete") },
            { QStringLiteral("requestId"), f.commands.values.last().value(QStringLiteral("requestId")) },
            { QStringLiteral("projectId"), test::id(11) } }, 0);
        QVERIFY(f.workspace.projects().selectedProjectId().isEmpty());
        QVERIFY(f.sessions.containsSession(test::id(1)));
    }
    void projectCreationNeedsOnlyNameAndAttachedTerminalsFollowCatalog()
    {
        Fixture f;
        f.workspace.projects().create(QStringLiteral("  Release work  "));
        QCOMPARE(f.commands.values.size(), 1);
        const auto created = f.commands.values.first();
        QCOMPARE(created.value(QStringLiteral("type")).toString(), QStringLiteral("project.create"));
        QCOMPARE(created.value(QStringLiteral("name")).toString(), QStringLiteral("Release work"));
        QCOMPARE(created.size(), 3);
        f.workspace.projects().create(QStringLiteral("   "));
        QCOMPARE(f.commands.values.size(), 1);
        auto terminal = test::session(1);
        terminal.insert(QStringLiteral("projectId"), test::id(10));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        f.workspace.projects().open(test::id(10));
        QCOMPARE(f.workspace.projects().sessionIds(), QStringList {test::id(1)});
        QSignalSpy changed(&f.workspace.projects(), &kodosi::ProjectsModel::selectionChanged);
        terminal.insert(QStringLiteral("projectId"), test::id(11));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        QVERIFY(!changed.isEmpty());
        QVERIFY(f.workspace.projects().sessionIds().isEmpty());
        f.workspace.projects().open(test::id(11));
        QCOMPARE(f.workspace.projects().sessionIds(), QStringList {test::id(1)});
        f.sessions.beginRefresh();
        QVERIFY(f.workspace.projects().sessionIds().isEmpty());
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.projects().sessionIds().isEmpty());
    }
    void truncatedProjectCatalogRetainsSelectionAndAllowsRecovery()
    {
        Fixture f;
        const QJsonObject project {{QStringLiteral("id"), test::id(11)},
            {QStringLiteral("name"), QStringLiteral("Beyond the page")},
            {QStringLiteral("ownerUserId"), test::id(50)}};
        f.workspace.projects().open(test::id(11));
        const auto previousRequest = f.commands.values.last().value(QStringLiteral("requestId"));
        QJsonObject snapshot {{QStringLiteral("type"), QStringLiteral("projects.snapshot")},
            {QStringLiteral("projects"), QJsonArray {}},
            {QStringLiteral("invitations"), QJsonArray {QJsonObject {
                {QStringLiteral("id"), test::id(12)}, {QStringLiteral("projectId"), test::id(10)},
                {QStringLiteral("projectName"), QStringLiteral("Invitation")}}}},
            {QStringLiteral("projectsTruncated"), true},
            {QStringLiteral("invitationsTruncated"), true}};
        f.workspace.apply(snapshot, 0);
        QVERIFY(f.workspace.projects().catalogTruncated());
        QCOMPARE(f.workspace.projects().selectedProjectId(), test::id(11));
        const auto detailRequest = f.commands.values.last();
        QCOMPARE(detailRequest.value(QStringLiteral("type")).toString(), QStringLiteral("project.open"));
        QCOMPARE(detailRequest.value(QStringLiteral("projectId")).toString(), test::id(11));
        QVERIFY(detailRequest.value(QStringLiteral("requestId")) != previousRequest);
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("project.result")},
            {QStringLiteral("operation"), QStringLiteral("project.open")},
            {QStringLiteral("requestId"), detailRequest.value(QStringLiteral("requestId"))},
            {QStringLiteral("project"), project}, {QStringLiteral("members"), QJsonArray {}}}, 0);
        QCOMPARE(f.workspace.projects().selectedProject().value(QStringLiteral("name")).toString(), QStringLiteral("Beyond the page"));
        f.workspace.projects().declineInvitation(test::id(12));
        auto command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("project.invitation.reject"));
        const auto sent = f.commands.values.size();
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("project.result")},
            {QStringLiteral("operation"), command.value(QStringLiteral("type"))},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))}}, 0);
        QCOMPARE(f.commands.values.size(), sent);
        f.workspace.projects().leave();
        command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("project.leave"));
        QCOMPARE(command.value(QStringLiteral("projectId")).toString(), test::id(11));
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("project.result")},
            {QStringLiteral("operation"), command.value(QStringLiteral("type"))},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))}}, 0);
        QVERIFY(f.workspace.projects().selectedProjectId().isEmpty());
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("project.leave"));
        snapshot.remove(QStringLiteral("projectsTruncated"));
        snapshot.remove(QStringLiteral("invitationsTruncated"));
        f.workspace.apply(snapshot, 0);
        QVERIFY(!f.workspace.projects().catalogTruncated());
        snapshot.insert(QStringLiteral("invitationsTruncated"), true);
        f.workspace.projects().open(test::id(11));
        f.workspace.apply(snapshot, 0);
        QVERIFY(f.workspace.projects().catalogTruncated());
        QVERIFY(f.workspace.projects().selectedProjectId().isEmpty());
        f.workspace.reset();
        QVERIFY(!f.workspace.projects().catalogTruncated());
    }
    void unavailableProjectOutsideTruncatedPageClearsOnlyCurrentDetail()
    {
        Fixture f;
        f.workspace.projects().open(test::id(10));
        const auto stale = f.commands.values.last();
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("projects.snapshot")},
            {QStringLiteral("projects"), QJsonArray {}}, {QStringLiteral("invitations"), QJsonArray {}},
            {QStringLiteral("projectsTruncated"), true}}, 0);
        const auto current = f.commands.values.last();
        const auto failure = [](const QJsonObject& command) {
            return QJsonObject {{QStringLiteral("type"), QStringLiteral("project.error")},
                {QStringLiteral("operation"), QStringLiteral("project.open")},
                {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))},
                {QStringLiteral("projectId"), command.value(QStringLiteral("projectId"))},
                {QStringLiteral("message"), QStringLiteral("Project unavailable")}};
        };
        f.workspace.apply(failure(stale), 0);
        QCOMPARE(f.workspace.projects().selectedProjectId(), test::id(10));
        f.workspace.apply(failure(current), 0);
        QVERIFY(f.workspace.projects().selectedProjectId().isEmpty());
        QVERIFY(f.workspace.projects().catalogTruncated());
        QVERIFY(!f.workspace.error().isEmpty());
    }
    void friendDeviceAndShareCommandsStaySeparate()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        f.workspace.apply(test::snapshot({ test::session(1), test::session(2, true, true) }), 0);
        QVERIFY(f.workspace.sessionActions().share(test::id(1), { test::id(51) }));
        auto command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("session.share"));
        QVERIFY(!command.contains(QStringLiteral("access")));
        QCOMPARE(command.value(QStringLiteral("userIds")).toArray().size(), 1);
        QVERIFY(!f.workspace.sessionActions().share(test::id(2), { test::id(51) }));
        QVERIFY(f.workspace.sessionActions().attachProject(test::id(2), test::id(10)));
        f.workspace.devices().approve(QStringLiteral("ABCD-EFGH-JKMN"));
        command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("code")).toString(), QStringLiteral("ABCD-EFGH-JKMN"));
        f.workspace.people().verify(QStringLiteral("friend"), QStringLiteral(" kodosi:friend:abcd \n"));
        command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("friends.verify"));
        QCOMPARE(command.value(QStringLiteral("invite")).toString(), QStringLiteral("kodosi:friend:abcd"));
        f.workspace.people().trust(QStringLiteral("friend"));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("friends.identity.trust"));
        f.workspace.account().deleteAccount();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("auth.deleteAccount"));
        const QJsonObject invite { { QStringLiteral("type"), QStringLiteral("friends.result") },
            { QStringLiteral("operation"), QStringLiteral("friends.invite") },
            { QStringLiteral("text"), QStringLiteral("kodosi:owner:abcd") },
            { QStringLiteral("accountUserId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } };
        QSignalSpy copied(&f.workspace.people(), &kodosi::PeopleModel::inviteCopied);
        f.workspace.apply(invite, 1);
        QCOMPARE(copied.count(), 0);
        f.workspace.people().copyInvite();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("friends.invite"));
        f.workspace.apply(invite, 1);
        f.workspace.apply(invite, 1);
        QCOMPARE(copied.count(), 1);
    }
    void accountDeletionWaitsForTheBrowserAndEndsSignedOutWithANote()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } }, 1);
        const auto page = QStringLiteral("https://auth.example/realms/kodosi/protocol/openid-connect/auth?kc_action=delete_account");
        const QJsonObject pending { { QStringLiteral("type"), QStringLiteral("auth.deletion_pending") },
            { QStringLiteral("confirmationUri"), page } };
        f.workspace.apply(pending, 1);
        QVERIFY(f.workspace.account().deleting());
        QCOMPARE(f.workspace.account().deletionUri(), page);
        f.workspace.account().cancelDeletion();
        QVERIFY(!f.workspace.account().deleting());
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("auth.deleteAccount.cancel"));
        f.workspace.apply(pending, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.error") },
            { QStringLiteral("operation"), QStringLiteral("auth.deleteAccount") },
            { QStringLiteral("message"), QStringLiteral("Kodosi stopped waiting for the confirmation.") } }, 1);
        QVERIFY(!f.workspace.account().deleting());
        QVERIFY(f.workspace.account().signedIn());
        f.workspace.apply(pending, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("reason"), QStringLiteral("accountDeleted") } }, 2);
        QVERIFY(!f.workspace.account().deleting());
        QVERIFY(!f.workspace.account().signedIn());
        QCOMPARE(f.workspace.error(), QStringLiteral("Your Kodosi account is deleted."));
    }
    void bannedAccountSignsOutWithTheReasonUntilItSignsInAgain()
    {
        Fixture f;
        const QJsonObject ready { { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } };
        f.workspace.apply(ready, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("reason"), QStringLiteral("accountBanned") } }, 2);
        QVERIFY(!f.workspace.account().signedIn());
        QCOMPARE(f.workspace.error(), QStringLiteral("This account is banned because it tried too many user names. "
                                                     "Ask the operator of this Kodosi server to remove the ban."));
        f.workspace.apply(ready, 3);
        QVERIFY(f.workspace.error().isEmpty());
    }
    void unapprovedDeviceShowsItsReasonUntilItIsApproved()
    {
        Fixture f;
        const QJsonObject unapproved { { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("enrolled"), false } };
        const auto reason = QStringLiteral("Approve this device from one of your existing devices.");
        f.workspace.apply(unapproved, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("devices.list") },
            { QStringLiteral("selfDeviceId"), test::id(60) },
            { QStringLiteral("localDeviceEnrolled"), false },
            { QStringLiteral("notice"), reason },
            { QStringLiteral("devices"), QJsonArray {} } }, 1);
        f.workspace.apply(unapproved, 1);
        QVERIFY(f.workspace.account().signedIn());
        QVERIFY(!f.workspace.devices().localDeviceEnrolled());
        QCOMPARE(f.workspace.devices().notice(), reason);
        QVERIFY(f.workspace.error().isEmpty());
        f.workspace.devices().startFresh();
        const auto command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("devices.reset"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("devices.list") },
            { QStringLiteral("selfDeviceId"), test::id(60) },
            { QStringLiteral("localDeviceEnrolled"), true },
            { QStringLiteral("devices"), QJsonArray {} } }, 1);
        QVERIFY(f.workspace.devices().localDeviceEnrolled());
        QVERIFY(f.workspace.devices().notice().isEmpty());
    }
    void aShownCodeFollowsThePhoneThatScansItAndEndsWhenTheCodeDoes()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } }, 1);
        auto& devices = f.workspace.devices();
        const auto resolved = [](const QString& outcome, const QJsonObject& more = {}) {
            QJsonObject event { { QStringLiteral("type"), QStringLiteral("devices.link.offerResolved") },
                { QStringLiteral("outcome"), outcome } };
            for (auto field = more.begin(); field != more.end(); ++field)
                event.insert(field.key(), field.value());
            return event;
        };
        const auto shown = [&] {
            devices.offer();
            const auto asked = f.commands.values.last();
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("devices.result") },
                { QStringLiteral("requestId"), asked.value(QStringLiteral("requestId")) },
                { QStringLiteral("operation"), QStringLiteral("devices.link.offer") },
                { QStringLiteral("qr"), QJsonArray { QStringLiteral("10"), QStringLiteral("01") } } }, 1);
        };

        f.workspace.apply(resolved(QStringLiteral("approved"), { { QStringLiteral("deviceLabel"), QStringLiteral("Other") } }), 1);
        QVERIFY(devices.offerState().isEmpty());

        shown();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("devices.link.offer"));
        QCOMPARE(devices.offerState(), QStringLiteral("shown"));
        QCOMPARE(devices.offerQr(), (QStringList { QStringLiteral("10"), QStringLiteral("01") }));
        f.workspace.apply(resolved(QStringLiteral("approved"), { { QStringLiteral("deviceLabel"), QStringLiteral("Pixel") } }), 1);
        QCOMPARE(devices.offerState(), QStringLiteral("approved"));
        QCOMPARE(devices.offerDetail(), QStringLiteral("Pixel"));
        devices.endOffer();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("devices.link.offer"));

        shown();
        f.workspace.apply(resolved(QStringLiteral("expired")), 1);
        QCOMPARE(devices.offerState(), QStringLiteral("ended"));

        shown();
        f.workspace.apply(resolved(QStringLiteral("cancelled"), { { QStringLiteral("message"), QStringLiteral("This account has 4 devices.") } }), 1);
        QCOMPARE(devices.offerState(), QStringLiteral("failed"));
        QCOMPARE(devices.offerDetail(), QStringLiteral("This account has 4 devices."));

        shown();
        devices.endOffer();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("devices.link.offer.cancel"));
        QVERIFY(devices.offerState().isEmpty());
        QVERIFY(f.workspace.error().isEmpty());
    }
    void aCodeThatCannotBeShownSaysWhyInItsDialog()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } }, 1);
        f.workspace.devices().offer();
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("devices.error") },
            { QStringLiteral("requestId"), f.commands.values.last().value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), QStringLiteral("devices.link.offer") },
            { QStringLiteral("message"), QStringLiteral("Kodosi could not reach the server.") } }, 1);
        QCOMPARE(f.workspace.devices().offerState(), QStringLiteral("failed"));
        QCOMPARE(f.workspace.devices().offerDetail(), QStringLiteral("Kodosi could not reach the server."));
        QVERIFY(f.workspace.error().isEmpty());
    }
    void dismissedDeviceRequestsStayHiddenUntilANewOneComes()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } }, 1);
        const auto request = [](const QString& id, const QString& label) {
            return QJsonObject { { QStringLiteral("requestId"), id }, { QStringLiteral("deviceLabel"), label } };
        };
        const auto snapshot = [](const QJsonArray& requests) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("devices.link.snapshot") },
                { QStringLiteral("requests"), requests } };
        };
        const auto laptop = request(test::id(70), QStringLiteral("Laptop"));
        f.workspace.apply(snapshot({ laptop }), 1);
        QCOMPARE(f.workspace.devices().waitingRequests().size(), 1);
        f.workspace.devices().dismissRequests();
        QVERIFY(f.workspace.devices().waitingRequests().isEmpty());
        f.workspace.apply(snapshot({ laptop, request(test::id(71), QStringLiteral("Desktop")) }), 1);
        const auto waiting = f.workspace.devices().waitingRequests();
        QCOMPARE(waiting.size(), 1);
        QCOMPARE(waiting.first().toMap().value(QStringLiteral("deviceLabel")).toString(), QStringLiteral("Desktop"));
        f.workspace.devices().dismissApprovalNotice();
        QVERIFY(f.workspace.devices().approvalNoticeDismissed());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 2);
        QVERIFY(f.workspace.devices().waitingRequests().isEmpty());
        QVERIFY(!f.workspace.devices().approvalNoticeDismissed());
    }
    void anotherDeviceIsNeitherThisOneNorARecoveryKey()
    {
        Fixture f;
        const auto listed = [](const QJsonArray& devices) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("devices.list") },
                { QStringLiteral("selfDeviceId"), test::id(60) }, { QStringLiteral("localDeviceEnrolled"), true },
                { QStringLiteral("devices"), devices } };
        };
        const auto device = [](int value, bool recoveryKey = false) {
            return QJsonObject { { QStringLiteral("deviceId"), test::id(value) },
                { QStringLiteral("recoveryKey"), recoveryKey } };
        };
        f.workspace.apply(listed({ device(60), device(61, true) }), 1);
        QVERIFY(!f.workspace.devices().hasOtherDevices());
        f.workspace.apply(listed({ device(60), device(61, true), device(62) }), 1);
        QVERIFY(f.workspace.devices().hasOtherDevices());
    }
    void kodosiIsOfflineUntilItReachesTheServerAgainOrTheAccountChanges()
    {
        Fixture f;
        f.workspace.apply(accountEvent(test::id(50), 1), 1);
        const auto connection = [](bool online) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("connection.state") },
                { QStringLiteral("online"), online } };
        };
        QSignalSpy changed(&f.workspace.account(), &kodosi::AccountModel::onlineChanged);
        f.workspace.apply(connection(false), 1);
        QVERIFY(!f.workspace.account().online());
        f.workspace.account().retryConnection();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("connection.retry"));
        f.workspace.apply(connection(true), 1);
        QVERIFY(f.workspace.account().online());
        QCOMPARE(changed.count(), 2);
        f.workspace.apply(connection(false), 1);
        f.workspace.apply(accountEvent({}, 2), 2);
        QVERIFY(f.workspace.account().online());
        QVERIFY(f.workspace.error().isEmpty());
    }
    void aSavedSignInThatComesBackIsNotShownAsSignedOut()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.restoring") },
            { QStringLiteral("reason"), QStringLiteral("offline") } }, 1);
        QCOMPARE(f.workspace.account().restoring(), QStringLiteral("offline"));
        QVERIFY(!f.workspace.account().signedIn());
        QVERIFY(!f.workspace.account().signingIn());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), QStringLiteral("user") } }, 1);
        QVERIFY(f.workspace.account().restoring().isEmpty());
        QVERIFY(f.workspace.account().signedIn());
    }
    void theAccountSaysWhereTheSignInIsKept()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.restoring") },
            { QStringLiteral("reason"), QStringLiteral("unreadable") },
            { QStringLiteral("credentialStore"), QStringLiteral("keychain") } }, 1);
        QCOMPARE(f.workspace.account().credentialStore(), QStringLiteral("keychain"));
        QVERIFY(f.workspace.account().credentialFolder().isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), QStringLiteral("user") },
            { QStringLiteral("credentialStore"), QStringLiteral("privateFile") },
            { QStringLiteral("credentialFolder"), QStringLiteral("/home/user/.config/kodosi/secrets") } }, 1);
        QCOMPARE(f.workspace.account().credentialStore(), QStringLiteral("privateFile"));
        QCOMPARE(f.workspace.account().credentialFolder(), QStringLiteral("/home/user/.config/kodosi/secrets"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 2);
        QVERIFY(f.workspace.account().credentialStore().isEmpty());
        QVERIFY(f.workspace.account().credentialFolder().isEmpty());
    }
    void pendingSignInCanBeCancelled()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 1);
        QVERIFY(!f.workspace.account().signingIn());
        f.workspace.account().login();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("browser")).toString(), QStringLiteral("system"));
        QSignalSpy opened(&f.workspace.account(), &kodosi::AccountModel::signInPageReady);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.browser") },
            { QStringLiteral("url"), QStringLiteral("https://example.invalid/auth?state=s") } }, 1);
        QVERIFY(f.workspace.account().signingIn());
        QCOMPARE(opened.count(), 1);
        f.workspace.account().cancelLogin();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("auth.login.cancel"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 2);
        QVERIFY(!f.workspace.account().signingIn());
        QVERIFY(f.workspace.account().signInPage().isEmpty());
    }
    void anAnswerInTheBrowserBringsTheWindowBackOnce()
    {
        Fixture f;
        QSignalSpy answered(&f.workspace.account(), &kodosi::AccountModel::browserAnswered);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.browser") },
            { QStringLiteral("url"), QStringLiteral("https://example.invalid/auth?state=s") } }, 1);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.finalizing") } }, 1);
        QCOMPARE(answered.count(), 1);
        QVERIFY(f.workspace.account().signingIn());
        QVERIFY(f.workspace.account().signInPage().isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.finalizing") } }, 1);
        QCOMPARE(answered.count(), 1);
    }
    void aMinimizedWindowComesBackAsItWas()
    {
        Fixture f;
        QWindow window;
        f.desktop.attachWindow(&window);
        window.show();
        window.setWindowStates(Qt::WindowMaximized | Qt::WindowMinimized);
        f.desktop.bringToFront();
        QCOMPARE(window.windowStates(), Qt::WindowStates(Qt::WindowMaximized));
        window.setWindowStates(Qt::WindowMinimized);
        window.hide();
        f.desktop.bringToFront();
        QVERIFY(window.isVisible());
        QCOMPARE(window.windowStates(), Qt::WindowStates(Qt::WindowNoState));
    }
    void theWindowPlaceIsSavedWhenTheWindowRestsClosesOrQuits()
    {
        QTemporaryDir dir;
        const auto saved = [&dir] {
            return QSettings(dir.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat)
                .value(QStringLiteral("window/geometry"))
                .toRect()
                .topLeft();
        };
        QWindow window;
        {
            Fixture f(dir.path());
            f.desktop.attachWindow(&window);
            window.show();
            for (int x = 10; x <= 50; x += 10) {
                window.setPosition(x, 20);
                QCoreApplication::processEvents();
            }
            QCOMPARE_NE(saved(), QPoint(50, 20));
            QTRY_COMPARE(saved(), QPoint(50, 20));
            window.setPosition(60, 20);
            QCoreApplication::processEvents();
        }
        QCOMPARE(saved(), QPoint(60, 20));
        Fixture f(dir.path());
        f.desktop.attachWindow(&window);
        window.setPosition(70, 20);
        QCoreApplication::processEvents();
        window.close();
        QCOMPARE(saved(), QPoint(70, 20));
    }
    void aProjectThatTheCommandShowsOpensInFront()
    {
        Fixture f;
        QWindow window;
        f.desktop.attachWindow(&window);
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.show") },
            { QStringLiteral("projectId"), test::id(10) } }, 0);
        QVERIFY(window.isVisible());
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Projects);
        QCOMPARE(f.workspace.projects().selectedProjectId(), test::id(10));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("project.open"));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("projectId")).toString(), test::id(10));
    }
    void endedSignInReturnsToSignedOutWithOneMessage()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) } }, 1);
        QVERIFY(f.workspace.account().signedIn());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("reason"), QStringLiteral("signedOut") } }, 2);
        QVERIFY(f.workspace.error().isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("reason"), QStringLiteral("expired") } }, 3);
        QVERIFY(!f.workspace.account().signedIn());
        QVERIFY(!f.workspace.error().isEmpty());
    }
    void exactAccountEpochChangeClearsStateAboveSignedRange()
    {
        Fixture f;
        const auto user = test::id(50);
        const QJsonObject ready {{QStringLiteral("type"), QStringLiteral("auth.ready")},
            {QStringLiteral("userId"), user}};
        f.workspace.apply(ready, std::numeric_limits<std::uint64_t>::max() - 1);
        f.workspace.apply(test::snapshot({test::session(1)}), std::numeric_limits<std::uint64_t>::max() - 1);
        QVERIFY(f.workspace.sessionActions().activate(test::id(1)));
        f.workspace.apply(ready, std::numeric_limits<std::uint64_t>::max());
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
    }
    void anErrorShowsAndEndsTheWorkOfItsRequest()
    {
        Fixture f;
        const auto error = [](const QString& requestId) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("session.error") },
                { QStringLiteral("operation"), QStringLiteral("session.rename") }, { QStringLiteral("message"), QStringLiteral("No") },
                { QStringLiteral("requestId"), requestId } };
        };
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QVERIFY(f.workspace.sessionActions().rename(test::id(1), QStringLiteral("Renamed")));
        f.workspace.apply(error(f.commands.values.last().value(QStringLiteral("requestId")).toString()), 0);
        QCOMPARE(f.workspace.error(), QStringLiteral("No"));
        QVERIFY(!f.workspace.sessionActions().busy());
        f.workspace.clearError();
        f.workspace.people().request(QStringLiteral("ada"));
        const auto friendRequest = f.commands.values.last().value(QStringLiteral("requestId")).toString();
        QVERIFY(!friendRequest.isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("friends.error") },
            { QStringLiteral("operation"), QStringLiteral("friends.request.send") },
            { QStringLiteral("message"), QStringLiteral("Nobody is called ada") },
            { QStringLiteral("requestId"), friendRequest } }, 0);
        QCOMPARE(f.workspace.error(), QStringLiteral("Nobody is called ada"));
    }

    void aRefreshThatTheAppAskedForFailsWithoutAnError()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("friends.error") },
            { QStringLiteral("operation"), QStringLiteral("friends.refresh") },
            { QStringLiteral("message"), QStringLiteral("Kodosi cannot reach the server. Check your connection and try again.") },
            { QStringLiteral("requestId"), test::id(1) } }, 0);
        QVERIFY(f.workspace.error().isEmpty());
    }

    void aReportAnswersInItsDialogOnlyForItsOwnRequest()
    {
        Fixture f;
        auto& people = f.workspace.people();
        QSignalSpy reported(&people, &kodosi::PeopleModel::reported);
        QSignalSpy failed(&people, &kodosi::PeopleModel::reportFailed);
        const auto answer = [](const QString& type, const QString& requestId) {
            return QJsonObject { { QStringLiteral("type"), type }, { QStringLiteral("operation"), QStringLiteral("friends.report") },
                { QStringLiteral("message"), QStringLiteral("Wait a few minutes.") }, { QStringLiteral("requestId"), requestId } };
        };
        people.report(QStringLiteral("ada"), QStringLiteral("Sent spam"));
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("friends.report"));
        QCOMPARE(sent.value(QStringLiteral("note")).toString(), QStringLiteral("Sent spam"));
        f.workspace.apply(answer(QStringLiteral("friends.error"), sent.value(QStringLiteral("requestId")).toString()), 0);
        QCOMPARE(failed.count(), 1);
        QCOMPARE(failed.first().first().toString(), QStringLiteral("Wait a few minutes."));
        QVERIFY(f.workspace.error().isEmpty());
        QVERIFY(!people.reporting());
        people.report(QStringLiteral("ada"), {});
        const auto again = f.commands.values.last().value(QStringLiteral("requestId")).toString();
        f.workspace.apply(answer(QStringLiteral("friends.result"), test::id(1)), 0);
        QVERIFY(people.reporting());
        f.workspace.apply(answer(QStringLiteral("friends.result"), again), 0);
        QCOMPARE(reported.count(), 1);
        QVERIFY(!people.reporting());
    }

    void aReportNoteEndsBeforeTheCharacterThatDoesNotFit()
    {
        Fixture f;
        QCOMPARE(f.workspace.people().clippedNote(QString(334, QChar(0x4E3E))), QString(333, QChar(0x4E3E)));
    }

    void aProjectAnswerBringsItsIssuesAndOlderMessages()
    {
        Fixture f;
        auto& projects = f.workspace.projects();
        projects.open(test::id(10));
        const auto message = [](int sequence) {
            return QJsonObject { { QStringLiteral("id"), test::id(100 + sequence) }, { QStringLiteral("sequence"), sequence },
                { QStringLiteral("authorId"), test::id(51) } };
        };
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.snapshot") },
            { QStringLiteral("content"), QJsonObject { { QStringLiteral("projectId"), test::id(10) }, { QStringLiteral("hasOlder"), true },
                { QStringLiteral("messages"), QJsonArray { message(5) } } } } }, 0);
        projects.act({ { QStringLiteral("type"), QStringLiteral("issues") }, { QStringLiteral("repositoryId"), test::id(20) } });
        const auto issues = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.result") }, { QStringLiteral("requestId"), issues },
            { QStringLiteral("operation"), QStringLiteral("project.act") }, { QStringLiteral("projectId"), test::id(10) },
            { QStringLiteral("action"), QStringLiteral("issues") }, { QStringLiteral("repositoryId"), test::id(20) },
            { QStringLiteral("issues"), QJsonArray { QJsonObject { { QStringLiteral("number"), 7 } } } } }, 0);
        QCOMPARE(projects.issues().size(), 1);
        projects.act({ { QStringLiteral("type"), QStringLiteral("read") }, { QStringLiteral("before"), 5 } });
        const auto older = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("project.result") }, { QStringLiteral("requestId"), older },
            { QStringLiteral("operation"), QStringLiteral("project.act") }, { QStringLiteral("projectId"), test::id(10) },
            { QStringLiteral("action"), QStringLiteral("read") },
            { QStringLiteral("content"), QJsonObject { { QStringLiteral("projectId"), test::id(10) }, { QStringLiteral("hasOlder"), false },
                { QStringLiteral("messages"), QJsonArray { message(3), message(4) } } } } }, 0);
        const auto messages = projects.content().value(QStringLiteral("messages")).toList();
        QCOMPARE(messages.size(), 3);
        QCOMPARE(messages.first().toMap().value(QStringLiteral("sequence")).toInt(), 3);
        QVERIFY(!projects.content().value(QStringLiteral("hasOlder")).toBool());
    }

    void aRejectedCommandShowsItsErrorAndLeavesNoWork()
    {
        Fixture f;
        f.commands.reject = true;
        QVERIFY(!f.workspace.sessionActions().create(QStringLiteral("Work"), QStringLiteral("/repo")));
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(!f.workspace.error().isEmpty());
    }
};
QTEST_MAIN(WorkspaceTest)
#include "tst_workspace.moc"
