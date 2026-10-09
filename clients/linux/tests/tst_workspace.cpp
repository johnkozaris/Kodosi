#include "SessionFixture.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonArray>
#include <QJsonDocument>
#include <QSignalSpy>
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
            return std::unexpected(kodosi::RuntimeFailure {
                kodosi::RuntimeFailure::Code::FfiRejected, 6, QStringLiteral("Busy") });
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
};
struct Fixture {
    QTemporaryDir dir;
    Commands commands;
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop {
        std::make_unique<QSettings>(dir.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat), false
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
        for (const auto* field : { "canSendInput", "canRetainFocus", "canSendFocus", "canResize", "isStageReady" })
            QVERIFY(!fields.contains(QLatin1String(field)));
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
        QCOMPARE(fields.value(QStringLiteral("headerTitle")).toString(), QStringLiteral("Terminal · GitHub Copilot"));
        const auto hosted = f.sessions.presentationForSession(test::id(2));
        QVERIFY(hosted.value(QStringLiteral("workingDirectory")).toString().isEmpty());
        QCOMPARE(hosted.value(QStringLiteral("displayDirectory")).toString(), QStringLiteral("/host/project"));
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
        const auto activity = [&](const int number) {
            return f.sessions.presentationForSession(test::id(number)).value(QStringLiteral("activity")).toString();
        };
        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("working")}}, QStringLiteral("Build")),
            reported(2, {{QStringLiteral("state"), QStringLiteral("idle")}}, QStringLiteral("\u2802 Build")),
        }), 0);
        QVERIFY(f.sessions.working());
        QVERIFY(f.sessions.presentationForSession(test::id(1)).value(QStringLiteral("working")).toBool());
        QVERIFY(!f.sessions.presentationForSession(test::id(2)).value(QStringLiteral("working")).toBool());
        QCOMPARE(activity(2), QStringLiteral("Build"));
        QVERIFY(f.sessions.attention().isEmpty());

        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("permission")},
                {QStringLiteral("title"), QStringLiteral("Review")}, {QStringLiteral("message"), QStringLiteral("Allow the command?")}}),
            reported(2, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("question")}}),
        }), 0);
        QVERIFY(!f.sessions.working());
        QCOMPARE(activity(1), QStringLiteral("Review: Allow the command?"));
        QCOMPARE(activity(2), QStringLiteral("Needs your answer"));
        QCOMPARE(f.sessions.attention(), (QStringList { test::id(1), test::id(2) }));

        f.sessions.clearAttention(test::id(1));
        f.sessions.clearAttention(test::id(2));
        f.workspace.apply(test::snapshot({
            reported(1, {{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("permission")}}),
            reported(2, {{QStringLiteral("state"), QStringLiteral("error")}}),
        }), 0);
        QCOMPARE(activity(2), QStringLiteral("Failed"));
        QCOMPARE(f.sessions.attention(), QStringList { test::id(2) });

        f.workspace.apply(test::snapshot({ reported(1, {{QStringLiteral("state"), QStringLiteral("dance")}}) }), 0);
        QCOMPARE(f.sessions.session(test::id(2))->programState, QStringLiteral("error"));
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
        f.workspace.people().copyInvite();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("friends.invite"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("friends.invite") },
            { QStringLiteral("text"), QStringLiteral("kodosi:owner:abcd") },
            { QStringLiteral("accountUserId"), test::id(50) }, { QStringLiteral("accountEpoch"), 1 } }, 1);
        QCOMPARE(f.workspace.people().invite(), QStringLiteral("kodosi:owner:abcd"));
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
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.device_code") },
            { QStringLiteral("userCode"), QStringLiteral("ABCD-EFGH") },
            { QStringLiteral("verificationUri"), QStringLiteral("https://example.invalid/device") } }, 1);
        QVERIFY(f.workspace.account().signingIn());
        f.workspace.account().cancelLogin();
        QCOMPARE(f.commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("auth.login.cancel"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("auth.required") } }, 2);
        QVERIFY(!f.workspace.account().signingIn());
        QVERIFY(f.workspace.account().userCode().isEmpty());
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
