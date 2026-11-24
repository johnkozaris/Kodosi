#include "SessionFixture.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonArray>
#include <algorithm>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest/QTest>

class Commands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
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
        f.workspace.apply(shown(QStringLiteral("Writing the fix")), 0);
        QCOMPARE(views.count(), 1);
        QCOMPARE(f.sessions.sessions().first().toMap().value(QStringLiteral("activity")).toString(), QStringLiteral("Writing the fix"));
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
    void folderGroupsKeepNamesAndRemoteDirectoriesPresentationOnly()
    {
        Fixture f;
        auto local = test::session(1);
        local.insert(QStringLiteral("workingDir"), QStringLiteral("/repo/project"));
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
    void aNotificationOpensItsTerminalOrTheOneThatStartedAgainOrElseTheList()
    {
        Fixture f;
        QObject dialog;
        f.desktop.setModalVisible(&dialog, true);
        QSignalSpy dismissed(&f.desktop, &kodosi::DesktopStateModel::dialogsDismissed);
        f.workspace.sessionActions().openNotified(test::id(1), {});
        QCOMPARE(dismissed.size(), 1);
        auto again = test::session(2);
        again.insert(QStringLiteral("restoredFrom"), test::id(9));
        auto friendsAgain = test::session(3, true, false);
        friendsAgain.insert(QStringLiteral("restoredFrom"), test::id(8));
        f.workspace.apply(test::snapshot({ test::session(1), again, friendsAgain }), 0);
        QCOMPARE(f.desktop.selectedSessionId(), test::id(1));

        f.desktop.setActiveView(kodosi::DesktopStateModel::Settings);
        f.workspace.sessionActions().openNotified(test::id(9), {});
        QCOMPARE(f.desktop.selectedSessionId(), test::id(2));
        QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Sessions);

        for (const int gone : { 8, 7 }) {
            f.desktop.setActiveView(kodosi::DesktopStateModel::Settings);
            f.desktop.setSidebarOpen(false);
            f.workspace.sessionActions().openNotified(test::id(gone), {});
            QCOMPARE(f.desktop.selectedSessionId(), test::id(2));
            QCOMPARE(f.desktop.activeView(), kodosi::DesktopStateModel::Sessions);
            QVERIFY(f.desktop.sidebarOpen());
        }
        QVERIFY(f.workspace.error().isEmpty());
        QVERIFY(f.desktop.lastError().isEmpty());
        QVERIFY(!opened(f.commands, test::id(8)) && !opened(f.commands, test::id(7)));
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
    void aRequestThatWaitsWhenTheAppFallsBehindEndsWithTheWordsAndTheChosenProjectStays()
    {
        Fixture f;
        const auto words = QStringLiteral("Kodosi cannot tell if that worked. Check before you try again.");
        f.workspace.projects().open(test::id(11));
        f.workspace.projects().rename(QStringLiteral("Plans"));
        QVERIFY(f.workspace.projects().busy());
        f.workspace.abandonPending(words);
        QVERIFY(!f.workspace.projects().busy());
        QCOMPARE(f.workspace.error(), words);
        QCOMPARE(f.workspace.projects().selectedProjectId(), test::id(11));
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
        f.workspace.people().verify(QStringLiteral("friend"), QStringLiteral(" kodosi:friend:abcd \n"));
        command = f.commands.values.last();
        QCOMPARE(command.value(QStringLiteral("type")).toString(), QStringLiteral("friends.verify"));
        QCOMPARE(command.value(QStringLiteral("invite")).toString(), QStringLiteral("kodosi:friend:abcd"));
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
    void aLocalTerminalShowsAProgramNoteUntilChatOpensOrThePersonDismissesIt()
    {
        Fixture f;
        const auto text = QStringLiteral("Copilot CLI has extensions turned off.");
        auto local = test::session(1);
        local.insert(QStringLiteral("program"), QStringLiteral("copilot"));
        local.insert(QStringLiteral("programNote"), text);
        auto chatting = test::session(2);
        chatting.insert(QStringLiteral("program"), QStringLiteral("copilot"));
        chatting.insert(QStringLiteral("programNote"), text);
        chatting.insert(QStringLiteral("face"), QStringLiteral("chat"));
        auto remote = test::session(3, true);
        remote.insert(QStringLiteral("program"), QStringLiteral("copilot"));
        remote.insert(QStringLiteral("programNote"), text);
        f.workspace.apply(test::snapshot({ local, chatting, remote }), 0);
        const auto note = [&](const int number) {
            return f.sessions.presentationForSession(test::id(number)).value(QStringLiteral("programNote")).toString();
        };
        QCOMPARE(note(1), text);
        QVERIFY(note(2).isEmpty());
        QVERIFY(note(3).isEmpty());
        f.desktop.dismissNote(test::id(1), text);
        QCOMPARE(f.desktop.dismissedNotes().value(test::id(1)).toString(), text);
        f.workspace.apply(test::snapshot({ chatting, remote }), 0);
        QVERIFY(f.desktop.dismissedNotes().isEmpty());
        auto invalid = test::session(4);
        invalid.insert(QStringLiteral("programNote"), 42);
        QTest::ignoreMessage(QtWarningMsg,
            "the app could not read the event sessions.snapshot: terminal 0 of its list is invalid or repeats");
        f.workspace.apply(test::snapshot({ invalid }), 0);
        QVERIFY(!f.sessions.containsSession(test::id(4)));
    }
};
QTEST_MAIN(WorkspaceTest)
#include "tst_workspace.moc"
