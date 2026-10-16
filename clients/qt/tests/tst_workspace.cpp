#include "SessionFixture.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonArray>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QFileInfo>
#include <QTemporaryDir>
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
    Commands commands;
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop {
        std::make_unique<QSettings>(dir.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat)
    };
    kodosi::DesktopSettings settings {
        std::make_unique<QSettings>(dir.filePath(QStringLiteral("terminal.ini")), QSettings::IniFormat)
    };
    kodosi::Workspace workspace { commands, sessions, desktop, settings };
    Fixture() { desktop.attachSessionCatalog(&sessions); }
};
class WorkspaceTest final : public QObject {
    Q_OBJECT
private slots:
    void roomChangesPublishTheMatchingDraftAndIdentity()
    {
        Fixture f;
        f.workspace.missions().open(test::id(10));
        f.workspace.missions().setPresentation(QStringLiteral("message"), QStringLiteral("Room ten draft"));
        f.workspace.missions().open(test::id(11));
        QString observedRoom, observedDraft;
        connect(&f.workspace.missions(), &kodosi::MissionsModel::roomChanged, &f.workspace, [&] {
            observedRoom = f.workspace.missions().selectedMissionId();
            observedDraft = f.workspace.missions().presentation().value(QStringLiteral("message")).toString();
        });
        f.workspace.missions().open(test::id(10));
        QCOMPARE(observedRoom, test::id(10));
        QCOMPARE(observedDraft, QStringLiteral("Room ten draft"));
    }

    void aRoomSendKeepsEditsMadeBeforeItsAcknowledgement()
    {
        Fixture f;
        f.workspace.missions().open(test::id(10));
        f.workspace.missions().setPresentation(QStringLiteral("message"), QStringLiteral("First message"));
        f.workspace.missions().roomAction({{QStringLiteral("type"), QStringLiteral("post")}, {QStringLiteral("text"), QStringLiteral("First message")}});
        const auto command = f.commands.values.last();
        f.workspace.missions().setPresentation(QStringLiteral("message"), QStringLiteral("Next draft"));
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("room.result")},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))},
            {QStringLiteral("operation"), QStringLiteral("room.command")},
            {QStringLiteral("roomId"), test::id(10)}, {QStringLiteral("action"), QStringLiteral("post")}}, 0);
        QCOMPARE(f.workspace.missions().presentation().value(QStringLiteral("message")).toString(), QStringLiteral("Next draft"));
        f.workspace.missions().open(test::id(11));
        f.workspace.missions().open(test::id(10));
        QCOMPARE(f.workspace.missions().presentation().value(QStringLiteral("message")).toString(), QStringLiteral("Next draft"));
    }

    void aRoomTerminalKeepsTheRoomVisible()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({test::session(1, true, false)}), 0);
        f.workspace.missions().open(test::id(10));
        f.desktop.setActiveView(kodosi::DesktopStateModel::Missions);
        QSignalSpy navigation(&f.desktop, &kodosi::DesktopStateModel::activeViewChanged);
        QVERIFY(f.workspace.sessionActions().activateInRoom(test::id(1)));
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Missions);
        QCOMPARE(f.workspace.missions().presentation().value(QStringLiteral("terminal")).toString(), test::id(1));
        f.desktop.selectSession(test::id(1));
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Missions);
        QCOMPARE(navigation.count(), 0);
    }

    void updatingRoomTasksPreservesTheirVisibleOrder()
    {
        Fixture f;
        f.workspace.missions().open(test::id(10));
        const auto task = [](int id, int version) {
            return QJsonObject {{QStringLiteral("id"), test::id(id)}, {QStringLiteral("version"), version}};
        };
        const auto room = [&](const QJsonArray& tasks) {
            f.workspace.apply({{QStringLiteral("type"), QStringLiteral("room.snapshot")},
                {QStringLiteral("room"), QJsonObject {{QStringLiteral("roomId"), test::id(10)},
                    {QStringLiteral("tasks"), tasks}, {QStringLiteral("messages"), QJsonArray {}}}}}, 0);
        };
        room({task(1,1), task(2,1)});
        room({task(2,1), task(1,2)});
        const auto tasks = f.workspace.missions().room().value(QStringLiteral("tasks")).toList();
        QCOMPARE(tasks.size(), 2);
        QCOMPARE(tasks.first().toMap().value(QStringLiteral("id")).toString(), test::id(1));
        QCOMPARE(tasks.first().toMap().value(QStringLiteral("version")).toInt(), 2);
    }

    void aRoomIsNewWhenSomeoneElseWroteInIt()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        const auto message = [](int sequence, const QString& author, const QString& agent = {}) {
            return QJsonObject { { QStringLiteral("id"), test::id(100 + sequence) }, { QStringLiteral("sequence"), sequence },
                { QStringLiteral("authorId"), author }, { QStringLiteral("agent"), agent.isEmpty() ? QJsonValue() : QJsonValue(agent) } };
        };
        const auto room = [&](const QJsonArray& messages) {
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("room.snapshot") },
                { QStringLiteral("room"), QJsonObject { { QStringLiteral("roomId"), test::id(10) },
                    { QStringLiteral("tasks"), QJsonArray {} }, { QStringLiteral("messages"), messages } } } }, 1);
        };
        auto& missions = f.workspace.missions();
        room({ message(5, test::id(51)) });
        QVERIFY(missions.unreadRooms().isEmpty());
        room({ message(3, test::id(51)), message(6, test::id(50)) });
        QVERIFY(missions.unreadRooms().isEmpty());
        room({ message(7, test::id(50), QStringLiteral("Claude Code")) });
        QCOMPARE(missions.unreadRooms(), QStringList { test::id(10) });

        f.desktop.setActiveView(kodosi::DesktopStateModel::Missions);
        missions.open(test::id(10));
        QVERIFY(missions.unreadRooms().isEmpty());
        room({ message(8, test::id(51)) });
        QVERIFY(missions.unreadRooms().isEmpty());
        missions.setPresentation(QStringLiteral("conversation"), false);
        room({ message(9, test::id(51)) });
        QCOMPARE(missions.unreadRooms(), QStringList { test::id(10) });
        missions.setPresentation(QStringLiteral("conversation"), true);
        QVERIFY(missions.unreadRooms().isEmpty());
    }
    void aRoomCountsWhatArrivedOutOfView()
    {
        Fixture f;
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.ready") },
            { QStringLiteral("userId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        auto& missions = f.workspace.missions();
        f.desktop.setActiveView(kodosi::DesktopStateModel::Missions);
        missions.open(test::id(10));
        int sequence = 0;
        QJsonArray tasks;
        QJsonArray repositories;
        const auto room = [&](int messages) {
            QJsonArray list;
            for (int index = 0; index < messages; ++index) {
                ++sequence;
                list.append(QJsonObject { { QStringLiteral("id"), test::id(100 + sequence) }, { QStringLiteral("sequence"), sequence },
                    { QStringLiteral("authorId"), test::id(51) } });
            }
            f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("room.snapshot") },
                { QStringLiteral("room"), QJsonObject { { QStringLiteral("roomId"), test::id(10) }, { QStringLiteral("tasks"), tasks },
                    { QStringLiteral("repositories"), repositories }, { QStringLiteral("messages"), list } } } }, 1);
        };
        const auto fresh = [&] { return missions.presentation().value(QStringLiteral("fresh")).toList(); };
        const auto unread = [&] { return missions.presentation().value(QStringLiteral("unread")).toInt(); };
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 1 } } };
        room(0);
        QVERIFY(fresh().isEmpty());
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 2 } } };
        room(0);
        QCOMPARE(fresh(), QVariantList { 1 });
        repositories = { QJsonObject { { QStringLiteral("id"), test::id(2) } } };
        room(0);
        QCOMPARE(fresh(), (QVariantList { 1, 2 }));
        missions.setPresentation(QStringLiteral("canvas"), 1);
        QCOMPARE(fresh(), QVariantList { 2 });
        tasks = { QJsonObject { { QStringLiteral("id"), test::id(1) }, { QStringLiteral("version"), 3 } } };
        room(0);
        QCOMPARE(fresh(), QVariantList { 2 });

        room(1);
        QCOMPARE(unread(), 0);
        missions.setPresentation(QStringLiteral("conversation"), false);
        room(2);
        room(1);
        QCOMPARE(unread(), 3);
        missions.setPresentation(QStringLiteral("conversation"), true);
        QCOMPARE(unread(), 0);
        missions.setPresentation(QStringLiteral("followLatest"), false);
        room(1);
        QCOMPARE(unread(), 1);
        missions.setPresentation(QStringLiteral("followLatest"), true);
        QCOMPARE(unread(), 0);
    }

    void aTitleThatOnlyAnimatesKeepsTheViews()
    {
        Fixture f;
        const auto titled = [](const QString& title) {
            auto entry = test::session(1);
            entry.insert(QStringLiteral("title"), title);
            return test::snapshot({ entry });
        };
        f.workspace.apply(titled(QStringLiteral("\u280b Reading the logs")), 0);
        QSignalSpy views(&f.sessions, &kodosi::SessionCatalogModel::folderGroupsChanged);
        QSignalSpy applied(&f.sessions, &kodosi::SessionCatalogModel::authoritativeSnapshotApplied);
        f.workspace.apply(titled(QStringLiteral("\u2819 Reading the logs")), 0);
        QCOMPARE(views.count(), 0);
        QCOMPARE(applied.count(), 1);
        QCOMPARE(f.sessions.sessions().first().toMap().value(QStringLiteral("activity")).toString(), QStringLiteral("Reading the logs"));
        QVERIFY(f.sessions.sessions().first().toMap().value(QStringLiteral("working")).toBool());
        f.workspace.apply(titled(QStringLiteral("\u2819 Writing the fix")), 0);
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
    void folderGroupsKeepNamesAndRemoteDirectoriesPresentationOnly()
    {
        Fixture f;
        auto local = test::session(1);
        local.insert(QStringLiteral("workingDir"), QStringLiteral("/repo/project"));
        local.insert(QStringLiteral("title"), QStringLiteral("GitHub Copilot"));
        auto remote = test::session(2, true, false);
        remote.insert(QStringLiteral("workingDir"), QStringLiteral("/host/project"));
        f.sessions.apply(test::snapshot({ local, remote }));
        QCOMPARE(f.sessions.folderGroups().size(), 2);
        const auto fields = f.sessions.presentationForSession(test::id(1));
        QCOMPARE(fields.value(QStringLiteral("name")).toString(), QStringLiteral("Terminal"));
        QCOMPARE(fields.value(QStringLiteral("activity")).toString(), QStringLiteral("GitHub Copilot"));
        const auto hosted = f.sessions.presentationForSession(test::id(2));
        QVERIFY(hosted.value(QStringLiteral("workingDirectory")).toString().isEmpty());
        QCOMPARE(hosted.value(QStringLiteral("displayDirectory")).toString(), QStringLiteral("/host/project"));
    }
    void detailsNameTheRoomOfATerminal()
    {
        Fixture f;
        auto inRoom = test::session(1);
        inRoom.insert(QStringLiteral("missionId"), test::id(10));
        inRoom.insert(QStringLiteral("missionName"), QStringLiteral("Release"));
        f.sessions.apply(test::snapshot({ inRoom }));
        QCOMPARE(f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("missionName")).toString(),
            QStringLiteral("Release"));
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
    void aTerminalNeedsYouAfterItsAgentStopsOrItsBellRings()
    {
        Fixture f;
        auto working = test::session(1);
        working.insert(QStringLiteral("title"), QStringLiteral("\u2802 Build"));
        f.workspace.apply(test::snapshot({ working, test::session(2) }), 0);
        QVERIFY(f.sessions.working());
        QVERIFY(f.sessions.attention().isEmpty());
        auto idle = test::session(1);
        idle.insert(QStringLiteral("title"), QStringLiteral("Build"));
        f.workspace.apply(test::snapshot({ idle, test::session(2) }), 0);
        QVERIFY(!f.sessions.working());
        QCOMPARE(f.sessions.attention(), QStringList { test::id(1) });
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("term.bell")},
            {QStringLiteral("sessionId"), test::id(2)}}, 0);
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("term.bell")},
            {QStringLiteral("sessionId"), test::id(9)}}, 0);
        QCOMPARE(f.sessions.attention(), (QStringList { test::id(1), test::id(2) }));
        f.sessions.clearAttention(test::id(1));
        QCOMPARE(f.sessions.attention(), QStringList { test::id(2) });
        f.workspace.apply(test::snapshot({ idle }), 0);
        QVERIFY(f.sessions.attention().isEmpty());
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
    void aStartLineGoesToTheTerminalAsOnePrintableLine()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);

        auto& actions = f.workspace.sessionActions();
        const auto sent = f.commands.values.size();
        QVERIFY(!actions.run(test::id(1), QStringLiteral("  ")));
        QVERIFY(!actions.run(test::id(1), QStringLiteral("claude\nrm -rf x")));
        QCOMPARE(f.commands.values.size(), sent);
        const auto line = QStringLiteral("claude --add-dir ~/notes-") + QString::fromUcs4(U"\U0001F4C1");
        QVERIFY(actions.run(test::id(1), QLatin1Char(' ') + line + QLatin1Char(' ')));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("session.run"));
        QCOMPARE(f.commands.values.last().value(QStringLiteral("command")).toString(), line);
    }
    void aProgramStatusReportIsTheStateOfItsTerminal()
    {
        Fixture f;
        const auto reported = [](const int number, const QJsonObject& status, const QString& title = {}) {
            auto session = test::session(number);
            session.insert(QStringLiteral("programStatus"), status);
            if (!title.isEmpty())
                session.insert(QStringLiteral("title"), title);
            return session;
        };
        const auto field = [&](const int number, const QString& name) {
            return f.sessions.presentationForSession(test::id(number)).value(name);
        };
        const auto activity = [&](const int number) { return field(number, QStringLiteral("activity")).toString(); };
        const auto sign = [&](const int number) { return field(number, QStringLiteral("sign")).toString(); };
        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("working")}, {QStringLiteral("progress"), 40}}, QStringLiteral("Build")),
            reported(2, {{QStringLiteral("state"), QStringLiteral("idle")}}, QStringLiteral("\u2802 Build")),
        }), 0);
        QVERIFY(f.sessions.working());
        QVERIFY(f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("working")).toBool());
        QVERIFY(!f.sessions.presentationForSession(test::id(2)).value(QStringLiteral("working")).toBool());
        QCOMPARE(activity(2), QStringLiteral("Build"));
        QCOMPARE(field(1, QStringLiteral("progress")).toInt(), 40);
        QCOMPARE(field(2, QStringLiteral("progress")).toInt(), -1);
        QVERIFY(sign(1).isEmpty());
        QVERIFY(f.sessions.attention().isEmpty());

        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("permission")},
                {QStringLiteral("title"), QStringLiteral("Review")}, {QStringLiteral("message"), QStringLiteral("Allow the command?")}}),
            reported(2, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("question")}}),
        }), 0);
        QVERIFY(!f.sessions.working());
        QCOMPARE(activity(1), QStringLiteral("Review: Allow the command?"));
        QVERIFY(activity(2).isEmpty());
        QCOMPARE(sign(1), QStringLiteral("hand"));
        QCOMPARE(sign(2), QStringLiteral("question"));
        QVERIFY(field(1, QStringLiteral("asks")).toBool());
        QCOMPARE(f.sessions.attention(), (QStringList { test::id(1), test::id(2) }));

        f.sessions.clearAttention(test::id(1));
        f.sessions.clearAttention(test::id(2));
        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("permission")}}),
            reported(2, {{QStringLiteral("state"), QStringLiteral("error")}}),
        }), 0);
        QVERIFY(activity(2).isEmpty());
        QCOMPARE(sign(2), QStringLiteral("failed"));
        QVERIFY(!field(2, QStringLiteral("asks")).toBool());
        QCOMPARE(f.sessions.attention(), QStringList { test::id(2) });

        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("auth")}}),
            reported(2, {{QStringLiteral("state"), QStringLiteral("done")}}),
        }), 0);
        QCOMPARE(sign(1), QStringLiteral("key"));
        QCOMPARE(sign(2), QStringLiteral("done"));
        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("working")}, {QStringLiteral("progress"), 101}}),
        }), 0);
        QCOMPARE(sign(2), QStringLiteral("done"));

        f.workspace.apply(test::snapshot({ reported(1, {{QStringLiteral("state"), QStringLiteral("dance")}}) }), 0);
        QCOMPARE(f.sessions.session(test::id(2))->programState, QStringLiteral("done"));
    }
    void aRoomTerminalThatIsNotOpenHereShowsTheStatusOfItsHost()
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
        auto working = closed(2);
        working.insert(QStringLiteral("programStatus"), QJsonObject { { QStringLiteral("state"), QStringLiteral("working") } });
        auto titled = closed(3);
        titled.insert(QStringLiteral("title"), QStringLiteral("\u2802 Build"));
        f.workspace.apply(test::snapshot({ asking, working, titled }), 0);
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
        QVERIFY(f.workspace.sessionActions().busy());
        QVERIFY(f.sessions.containsSession(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().contains(test::id(1)));
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(!f.sessions.containsSession(test::id(1)));
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
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
    void createsOnlyAfterCorrelatedSnapshot()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.sessionActions().create(QStringLiteral("Work"), QStringLiteral("/repo")));
        QVERIFY(f.desktop.selectedSessionId().isEmpty());
        auto created = test::session(1);
        created.insert(
            QStringLiteral("createRequestId"), f.commands.values.last().value(QStringLiteral("requestId")));
        f.workspace.apply(test::snapshot({ created }), 0);
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));
        QVERIFY(!f.workspace.sessionActions().busy());
    }
    void aTerminalStartedForARoomJoinsThatRoom()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.sessionActions().create(
            {}, QStringLiteral("/repo"), QStringLiteral("claude"), QStringLiteral(" fix the test "), test::id(7)));
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("session.create"));
        QCOMPARE(sent.value(QStringLiteral("missionId")).toString(), test::id(7));
        QCOMPARE(sent.value(QStringLiteral("command")).toString(), QStringLiteral("claude"));
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
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") },
            { QStringLiteral("accountEpoch"), 2 } }, 2);
        QVERIFY(f.desktop.stagedSessionIds().isEmpty());
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(!f.workspace.account().signedIn());
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
    void missionRepliesAreRequestScopedAndDeleteKeepsTerminal()
    {
        Fixture f;
        auto terminal = test::session(1);
        terminal.insert(QStringLiteral("missionId"), test::id(11));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        f.workspace.missions().open(test::id(10));
        const auto oldRequest = f.commands.values.last().value(QStringLiteral("requestId"));
        f.workspace.missions().open(test::id(11));
        const auto request = f.commands.values.last().value(QStringLiteral("requestId"));
        const QJsonObject mission { { QStringLiteral("id"), test::id(11) },
            { QStringLiteral("name"), QStringLiteral("Project") },
            { QStringLiteral("ownerUserId"), test::id(50) } };
        QJsonObject reply { { QStringLiteral("type"), QStringLiteral("mission.snapshot") },
            { QStringLiteral("requestId"), oldRequest }, { QStringLiteral("mission"), mission },
            { QStringLiteral("members"), QJsonArray {} } };
        f.workspace.apply(reply, 0);
        QVERIFY(f.workspace.missions().selectedMission().isEmpty());
        reply.insert(QStringLiteral("requestId"), request);
        f.workspace.apply(reply, 0);
        QCOMPARE(f.workspace.missions().sessionIds(), QStringList { test::id(1) });
        f.workspace.apply({{QStringLiteral("type"),QStringLiteral("missions.snapshot")},{QStringLiteral("missions"),QJsonArray{}},{QStringLiteral("invitations"),QJsonArray{}}}, 0);
        QVERIFY(f.workspace.missions().selectedMissionId().isEmpty());
        f.workspace.missions().open(test::id(11));
        f.workspace.missions().remove();
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("mission.result") },
            { QStringLiteral("operation"), QStringLiteral("mission.delete") },
            { QStringLiteral("requestId"), f.commands.values.last().value(QStringLiteral("requestId")) },
            { QStringLiteral("missionId"), test::id(11) } }, 0);
        QVERIFY(f.workspace.missions().selectedMissionId().isEmpty());
        QVERIFY(f.sessions.containsSession(test::id(1)));
    }
    void missionCreationNeedsOnlyNameAndAttachedTerminalsFollowCatalog()
    {
        Fixture f;
        f.workspace.missions().create(QStringLiteral("  Release work  "));
        QCOMPARE(f.commands.values.size(), 1);
        const auto created = f.commands.values.first();
        QCOMPARE(created.value(QStringLiteral("type")).toString(), QStringLiteral("mission.create"));
        QCOMPARE(created.value(QStringLiteral("name")).toString(), QStringLiteral("Release work"));
        QCOMPARE(created.size(), 3);
        f.workspace.missions().create(QStringLiteral("   "));
        QCOMPARE(f.commands.values.size(), 1);
        auto terminal = test::session(1);
        terminal.insert(QStringLiteral("missionId"), test::id(10));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        f.workspace.missions().open(test::id(10));
        QCOMPARE(f.workspace.missions().sessionIds(), QStringList {test::id(1)});
        QSignalSpy changed(&f.workspace.missions(), &kodosi::MissionsModel::selectionChanged);
        terminal.insert(QStringLiteral("missionId"), test::id(11));
        f.workspace.apply(test::snapshot({ terminal }), 0);
        QVERIFY(!changed.isEmpty());
        QVERIFY(f.workspace.missions().sessionIds().isEmpty());
        f.workspace.missions().open(test::id(11));
        QCOMPARE(f.workspace.missions().sessionIds(), QStringList {test::id(1)});
        f.sessions.beginRefresh();
        QVERIFY(f.workspace.missions().sessionIds().isEmpty());
        f.workspace.apply(test::snapshot({}), 0);
        QVERIFY(f.workspace.missions().sessionIds().isEmpty());
    }
    void truncatedMissionCatalogRetainsSelectionAndAllowsRecovery()
    {
        Fixture f;
        const QJsonObject mission {{QStringLiteral("id"), test::id(11)},
            {QStringLiteral("name"), QStringLiteral("Beyond the page")},
            {QStringLiteral("ownerUserId"), test::id(50)}};
        f.workspace.missions().open(test::id(11));
        const auto previousRequest = f.commands.values.last().value(QStringLiteral("requestId"));
        QJsonObject snapshot {{QStringLiteral("type"), QStringLiteral("missions.snapshot")},
            {QStringLiteral("missions"), QJsonArray {}},
            {QStringLiteral("invitations"), QJsonArray {QJsonObject {
                {QStringLiteral("id"), test::id(12)}, {QStringLiteral("missionId"), test::id(10)},
                {QStringLiteral("missionName"), QStringLiteral("Invitation")}}}},
            {QStringLiteral("missionsTruncated"), true},
            {QStringLiteral("invitationsTruncated"), true}};
        f.workspace.apply(snapshot, 0);
        QVERIFY(f.workspace.missions().catalogTruncated());
        QCOMPARE(f.workspace.missions().selectedMissionId(), test::id(11));
        const auto detailRequest = f.commands.values.last();
        QCOMPARE(detailRequest.value(QStringLiteral("type")).toString(), QStringLiteral("mission.open"));
        QCOMPARE(detailRequest.value(QStringLiteral("missionId")).toString(), test::id(11));
        QVERIFY(detailRequest.value(QStringLiteral("requestId")) != previousRequest);
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("mission.snapshot")},
            {QStringLiteral("requestId"), detailRequest.value(QStringLiteral("requestId"))},
            {QStringLiteral("mission"), mission}, {QStringLiteral("members"), QJsonArray {}}}, 0);
        QCOMPARE(f.workspace.missions().selectedMission().value(QStringLiteral("name")).toString(), QStringLiteral("Beyond the page"));
        f.workspace.missions().declineInvitation(test::id(12));
        auto command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("mission.invitation.reject"));
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("mission.result")},
            {QStringLiteral("operation"), command.value(QStringLiteral("type"))},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))}}, 0);
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("mission.list"));
        f.workspace.missions().leave();
        command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("mission.leave"));
        QCOMPARE(command.value(QStringLiteral("missionId")).toString(), test::id(11));
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("mission.result")},
            {QStringLiteral("operation"), command.value(QStringLiteral("type"))},
            {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))}}, 0);
        QVERIFY(f.workspace.missions().selectedMissionId().isEmpty());
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("mission.list"));
        snapshot.remove(QStringLiteral("missionsTruncated"));
        snapshot.remove(QStringLiteral("invitationsTruncated"));
        f.workspace.apply(snapshot, 0);
        QVERIFY(!f.workspace.missions().catalogTruncated());
        snapshot.insert(QStringLiteral("invitationsTruncated"), true);
        f.workspace.missions().open(test::id(11));
        f.workspace.apply(snapshot, 0);
        QVERIFY(f.workspace.missions().catalogTruncated());
        QVERIFY(f.workspace.missions().selectedMissionId().isEmpty());
        f.workspace.reset();
        QVERIFY(!f.workspace.missions().catalogTruncated());
    }
    void unavailableMissionOutsideTruncatedPageClearsOnlyCurrentDetail()
    {
        Fixture f;
        f.workspace.missions().open(test::id(10));
        const auto stale = f.commands.values.last();
        f.workspace.apply({{QStringLiteral("type"), QStringLiteral("missions.snapshot")},
            {QStringLiteral("missions"), QJsonArray {}}, {QStringLiteral("invitations"), QJsonArray {}},
            {QStringLiteral("missionsTruncated"), true}}, 0);
        const auto current = f.commands.values.last();
        const auto failure = [](const QJsonObject& command) {
            return QJsonObject {{QStringLiteral("type"), QStringLiteral("mission.error")},
                {QStringLiteral("operation"), QStringLiteral("mission.open")},
                {QStringLiteral("requestId"), command.value(QStringLiteral("requestId"))},
                {QStringLiteral("missionId"), command.value(QStringLiteral("missionId"))},
                {QStringLiteral("message"), QStringLiteral("Mission unavailable")}};
        };
        f.workspace.apply(failure(stale), 0);
        QCOMPARE(f.workspace.missions().selectedMissionId(), test::id(10));
        f.workspace.apply(failure(current), 0);
        QVERIFY(f.workspace.missions().selectedMissionId().isEmpty());
        QVERIFY(f.workspace.missions().catalogTruncated());
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
        QVERIFY(f.workspace.sessionActions().attachMission(test::id(2), test::id(10)));
        f.workspace.devices().approve(QStringLiteral("ABCD-EFGH-JKMN"));
        command = f.commands.values.last();
        QVERIFY(!command.contains(QStringLiteral("requestId")));
        QCOMPARE(command.value(QStringLiteral("code")).toString(), QStringLiteral("ABCD-EFGH-JKMN"));
        f.workspace.people().request(QStringLiteral("friend"));
        QVERIFY(f.commands.values.last().contains(QStringLiteral("requestId")));
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
        const QJsonObject invite { { QStringLiteral("type"), QStringLiteral("friends.invite") },
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
        QVERIFY(!command.contains(QStringLiteral("requestId")));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("devices.list") },
            { QStringLiteral("selfDeviceId"), test::id(60) },
            { QStringLiteral("localDeviceEnrolled"), true },
            { QStringLiteral("devices"), QJsonArray {} } }, 1);
        QVERIFY(f.workspace.devices().localDeviceEnrolled());
        QVERIFY(f.workspace.devices().notice().isEmpty());
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
    void anErrorShowsOnlyForARequestOfThisApp()
    {
        Fixture f;
        const auto error = [](const QString& requestId) {
            QJsonObject event { { QStringLiteral("type"), QStringLiteral("session.error") },
                { QStringLiteral("operation"), QStringLiteral("session.rename") }, { QStringLiteral("message"), QStringLiteral("No") } };
            if (!requestId.isEmpty())
                event.insert(QStringLiteral("requestId"), requestId);
            return event;
        };
        f.workspace.apply(error(test::id(70)), 0);
        QVERIFY(f.workspace.error().isEmpty());
        f.workspace.apply(error({}), 0);
        QCOMPARE(f.workspace.error(), QStringLiteral("No"));
        f.workspace.clearError();
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

    void strictRefreshHasNoRequestId()
    {
        Fixture f;
        f.workspace.sessionActions().refresh();
        QVERIFY(!f.commands.values.first().contains(QStringLiteral("requestId")));
        f.commands.reject = true;
        QVERIFY(!f.workspace.sessionActions().create(QStringLiteral("Work"), QStringLiteral("/repo")));
        QVERIFY(!f.workspace.sessionActions().busy());
        QVERIFY(!f.workspace.error().isEmpty());
    }
};
QTEST_GUILESS_MAIN(WorkspaceTest)
#include "tst_workspace.moc"
