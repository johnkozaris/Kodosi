#include "SessionFixture.hpp"
#include "presentation/AgentLauncherModel.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include <QJsonDocument>
#include <QSignalSpy>
#include <QtTest/QTest>

class ProviderCommands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    QByteArray lastBytes;
    Result send(QByteArrayView bytes) override
    {
        lastBytes = bytes.toByteArray();
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    QJsonObject reply(QJsonObject result, qsizetype index = -1) const
    {
        const auto command = values.at(index < 0 ? values.size() - 1 : index);
        return { { QStringLiteral("type"), QStringLiteral("provider.result") },
            { QStringLiteral("requestId"), command.value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), command.value(QStringLiteral("type")) },
            { QStringLiteral("result"), result } };
    }
};
void apply(kodosi::ConversationHistoryModel& history, const QJsonObject& event)
{
    history.apply(event, QJsonDocument(event).toJson(QJsonDocument::Compact));
}

class ProviderModelsTest final : public QObject {
    Q_OBJECT
private slots:
    void previewUsesByteCursorAndIgnoresStaleSelection()
    {
        ProviderCommands commands;
        kodosi::ConversationHistoryModel tools(commands);
        tools.open(QStringLiteral("claude"), QStringLiteral("/repo"));
        const QJsonObject item { { QStringLiteral("provider"), QStringLiteral("claude") },
            { QStringLiteral("workingDirectory"), QStringLiteral("/repo") },
            { QStringLiteral("nativeConversationId"), test::id(1) } };
        apply(tools, commands.reply({ { QStringLiteral("items"), QJsonArray { item } } }));
        QCOMPARE(tools.conversations().size(), 1);
        tools.preview(test::id(1));
        apply(tools, commands.reply(
            { { QStringLiteral("entries"),
                  QJsonArray { QJsonObject { { QStringLiteral("role"), QStringLiteral("user") },
                      { QStringLiteral("content"), QStringLiteral("Hello") } } } },
                { QStringLiteral("nextBeforeByte"), 123 } }));
        QCOMPARE(tools.entries().size(), 1);
        QVERIFY(tools.hasOlder());
        tools.loadOlder();
        QCOMPARE(commands.values.last().value(QStringLiteral("beforeByte")).toInt(), 123);
        QVERIFY(!commands.values.last().contains(QStringLiteral("beforeLine")));
        const auto stale = commands.reply({ { QStringLiteral("entries"), QJsonArray { QJsonObject {} } } });
        tools.open(QStringLiteral("copilot"), QStringLiteral("/other"));
        apply(tools, stale);
        QVERIFY(tools.entries().isEmpty());
        QVERIFY(tools.busy());
        QCOMPARE(commands.values.last().value(QStringLiteral("provider")).toString(),
            QStringLiteral("copilot"));
    }
    void historyReplacesPagesAndCommitsNavigationOnlyOnSuccess()
    {
        ProviderCommands commands;
        kodosi::ConversationHistoryModel tools(commands);
        tools.open(QStringLiteral("claude"), QStringLiteral("/repo"));
        apply(tools, commands.reply({ { QStringLiteral("items"), QJsonArray { QJsonObject {
            { QStringLiteral("provider"), QStringLiteral("claude") },
            { QStringLiteral("workingDirectory"), QStringLiteral("/repo") },
            { QStringLiteral("nativeConversationId"), test::id(1) } } } } }));
        tools.preview(test::id(1));
        QCOMPARE(commands.values.last().value(QStringLiteral("limit")).toInt(), 50);
        QCOMPARE(commands.values.last().value(QStringLiteral("maxBytes")).toInt(), 131072);
        const auto page = [](QString content, int before) {
            return QJsonObject { { QStringLiteral("entries"), QJsonArray { QJsonObject {
                { QStringLiteral("content"), content } } } }, { QStringLiteral("nextBeforeByte"), before } };
        };
        apply(tools, commands.reply(page(QStringLiteral("latest"), 1000)));
        for (int i = 0; i < 45; ++i) {
            tools.loadOlder();
            apply(tools, commands.reply(page(QString::number(i), 999 - i)));
            QCOMPARE(tools.entries().size(), 1);
        }
        QVERIFY(tools.hasNewer());
        tools.loadLatest();
        apply(tools, { { QStringLiteral("type"), QStringLiteral("provider.error") },
            { QStringLiteral("requestId"), commands.values.last().value(QStringLiteral("requestId")) },
            { QStringLiteral("message"), QStringLiteral("Unavailable") } });
        QCOMPARE(tools.entries().first().toMap().value(QStringLiteral("content")).toString(), QStringLiteral("44"));
        QVERIFY(tools.hasNewer());
        tools.loadNewer();
        QCOMPARE(commands.values.last().value(QStringLiteral("beforeByte")).toInt(), 957);
        apply(tools, commands.reply(page(QStringLiteral("43"), 956)));
        tools.loadLatest();
        QVERIFY(!commands.values.last().contains(QStringLiteral("beforeByte")));
        apply(tools, commands.reply(page(QStringLiteral("latest"), 1000)));
        QVERIFY(!tools.hasNewer());
        QCOMPARE(tools.entries().size(), 1);
    }
    void allProjectsPreviewUsesTheSelectedNativeDirectory()
    {
        ProviderCommands commands;
        kodosi::ConversationHistoryModel tools(commands);
        tools.open(QStringLiteral("claude"), {});
        QVERIFY(!commands.values.last().contains(QStringLiteral("workingDirectory")));
        apply(tools, commands.reply({ { QStringLiteral("items"), QJsonArray { QJsonObject {
            { QStringLiteral("provider"), QStringLiteral("claude") },
            { QStringLiteral("workingDirectory"), QStringLiteral("/another-project") },
            { QStringLiteral("nativeConversationId"), test::id(1) } } } } }));
        tools.preview(test::id(1));
        QCOMPARE(commands.values.last().value(QStringLiteral("workingDirectory")).toString(), QStringLiteral("/another-project"));
    }
    void rejectsConversationFromAnotherProject()
    {
        ProviderCommands commands;
        kodosi::ConversationHistoryModel tools(commands);
        tools.open(QStringLiteral("claude"), QStringLiteral("/repo"));
        apply(tools, commands.reply({ { QStringLiteral("items"),
            QJsonArray { QJsonObject { { QStringLiteral("provider"), QStringLiteral("claude") },
                { QStringLiteral("workingDirectory"), QStringLiteral("/other") },
                { QStringLiteral("nativeConversationId"), test::id(1) } } } } }));
        QVERIFY(tools.conversations().isEmpty());
        QVERIFY(!tools.error().isEmpty());
    }
    void preservesUnsignedByteCursor()
    {
        ProviderCommands commands;
        kodosi::ConversationHistoryModel tools(commands);
        tools.open(QStringLiteral("claude"), QStringLiteral("/repo"));
        const QJsonObject item {
            {QStringLiteral("provider"), QStringLiteral("claude")},
            {QStringLiteral("workingDirectory"), QStringLiteral("/repo")},
            {QStringLiteral("nativeConversationId"), test::id(1)}};
        apply(tools, commands.reply({{QStringLiteral("items"), QJsonArray {item}}}));
        tools.preview(test::id(1));
        auto reply = commands.reply({{QStringLiteral("entries"), QJsonArray {}},
            {QStringLiteral("nextBeforeByte"), QStringLiteral("18446744073709551615")}});
        auto bytes = QJsonDocument(reply).toJson(QJsonDocument::Compact);
        bytes.replace("\"18446744073709551615\"", "18446744073709551615");
        tools.apply(QJsonDocument::fromJson(bytes).object(), bytes);
        QVERIFY(tools.hasOlder());
        tools.loadOlder();
        QVERIFY(commands.lastBytes.contains("\"beforeByte\":18446744073709551615"));
    }
    void inspectionOnlyRequestsMetadata()
    {
        ProviderCommands commands;
        kodosi::ProviderFilesModel files(commands);
        files.inspect(QStringLiteral("claude"), QStringLiteral("/repo"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("provider.inspect"));
        QVERIFY(!commands.values.last().contains(QStringLiteral("content")));
        files.apply(commands.reply({ { QStringLiteral("provider"), QStringLiteral("claude") },
            { QStringLiteral("files"), QJsonArray {} } }));
        QVERIFY(!files.busy());
    }
    void launcherTakesItsOwnRepliesAndKeepsAChangeMadeAfterAQuestion()
    {
        ProviderCommands commands;
        kodosi::AgentLauncherModel launcher(commands);
        launcher.refresh(false);
        const auto asked = commands.values.size() - 1;
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("provider.agents"));
        auto foreign = commands.reply({ { QStringLiteral("agents"), QJsonArray { QJsonObject {} } } });
        foreign.insert(QStringLiteral("requestId"), test::id(9));
        launcher.apply(foreign);
        QVERIFY(launcher.agents().isEmpty());
        launcher.chooseAgent(QStringLiteral("claude"));
        QCOMPARE(commands.values.last().value(QStringLiteral("change")).toObject(),
            QJsonObject({ { QStringLiteral("type"), QStringLiteral("tab") }, { QStringLiteral("agent"), QStringLiteral("claude") } }));
        const QJsonObject claude { { QStringLiteral("agent"), QStringLiteral("claude") },
            { QStringLiteral("program"), QStringLiteral("claude") }, { QStringLiteral("installed"), true },
            { QStringLiteral("accounts"), QJsonArray {} }, { QStringLiteral("install"), QStringLiteral("curl") },
            { QStringLiteral("addsAccounts"), true } };
        const QJsonObject unchosen { { QStringLiteral("agent"), QJsonValue() }, { QStringLiteral("picks"), QJsonArray {} } };
        launcher.apply(commands.reply({ { QStringLiteral("agents"), QJsonArray { claude } }, { QStringLiteral("choice"), unchosen } }, asked));
        QCOMPARE(launcher.agents().size(), 1);
        QCOMPARE(launcher.choice().value(QStringLiteral("agent")).toString(), QStringLiteral("claude"));

        launcher.addAccount(QStringLiteral("claude"), QStringLiteral(" work "));
        const auto adding = commands.values.size() - 1;
        QCOMPARE(commands.values.last().value(QStringLiteral("name")).toString(), QStringLiteral("work"));
        launcher.refresh(true);
        const auto older = commands.values.size() - 1;
        launcher.apply(commands.reply({ { QStringLiteral("name"), QStringLiteral("work") },
            { QStringLiteral("command"), QStringLiteral("env CLAUDE_CONFIG_DIR=/home/ada/.claude-work claude") } }, adding));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("provider.choose"));
        QCOMPARE(commands.values.last().value(QStringLiteral("change")).toObject().value(QStringLiteral("account")).toString(),
            QStringLiteral("work"));
        QCOMPARE(launcher.agents().first().toMap().value(QStringLiteral("accounts")).toList().size(), 1);
        QCOMPARE(commands.values.size(), older + 2);
        launcher.apply(commands.reply({ { QStringLiteral("agents"), QJsonArray { claude } } }, older));
        QCOMPARE(commands.values.size(), older + 3);
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("provider.agents"));
    }
    void aPlanStartsTheShownAccountWithTheModelAndEffortTheAgentOffers()
    {
        ProviderCommands commands;
        kodosi::AgentLauncherModel launcher(commands);
        const QJsonObject claude { { QStringLiteral("agent"), QStringLiteral("claude") },
            { QStringLiteral("program"), QStringLiteral("claude") }, { QStringLiteral("installed"), true },
            { QStringLiteral("accounts"),
                QJsonArray { QJsonObject { { QStringLiteral("name"), QStringLiteral("default") },
                                 { QStringLiteral("command"), QStringLiteral("claude") },
                                 { QStringLiteral("identity"), QStringLiteral("ada@example.com") } },
                    QJsonObject { { QStringLiteral("name"), QStringLiteral("work") },
                        { QStringLiteral("command"), QStringLiteral("env CLAUDE_CONFIG_DIR=/home/ada/.claude-work claude") } } } },
            { QStringLiteral("install"), QStringLiteral("curl -fsSL https://claude.ai/install.sh | bash") },
            { QStringLiteral("addsAccounts"), true } };
        const QJsonObject codex { { QStringLiteral("agent"), QStringLiteral("codex") },
            { QStringLiteral("program"), QStringLiteral("codex") }, { QStringLiteral("installed"), false },
            { QStringLiteral("accounts"), QJsonArray {} }, { QStringLiteral("install"), QStringLiteral("npm install -g @openai/codex") },
            { QStringLiteral("addsAccounts"), true } };
        const QJsonObject choice { { QStringLiteral("agent"), QStringLiteral("claude") },
            { QStringLiteral("picks"),
                QJsonArray { QJsonObject { { QStringLiteral("agent"), QStringLiteral("claude") },
                    { QStringLiteral("account"), QStringLiteral("work") }, { QStringLiteral("model"), QStringLiteral("opus") },
                    { QStringLiteral("effort"), QStringLiteral("max") } } } } };
        launcher.refresh(true);
        launcher.apply(commands.reply({ { QStringLiteral("agents"), QJsonArray { claude, codex } }, { QStringLiteral("choice"), choice } }));
        const auto plan = [&](const QString& agent) { return launcher.plans().value(agent).toMap(); };
        const auto start = [&](const QString& agent) { return plan(agent).value(QStringLiteral("start")).toMap(); };
        const auto starts = [](const QString& account, const QString& model, const QString& effort) {
            return QVariantMap { { QStringLiteral("agent"), QStringLiteral("claude") }, { QStringLiteral("account"), account },
                { QStringLiteral("model"), model }, { QStringLiteral("effort"), effort } };
        };
        QCOMPARE(start(QStringLiteral("claude")), starts(QStringLiteral("work"), QStringLiteral("opus"), QStringLiteral("max")));
        QVERIFY(start(QStringLiteral("codex")).isEmpty());

        launcher.loadModels(QStringLiteral("claude"));
        launcher.apply(commands.reply({ { QStringLiteral("models"),
                                            QJsonArray { QJsonObject { { QStringLiteral("id"), QStringLiteral("opus") },
                                                { QStringLiteral("label"), QStringLiteral("Opus") },
                                                { QStringLiteral("efforts"), QJsonArray { QStringLiteral("low"), QStringLiteral("high") } } } } },
            { QStringLiteral("efforts"),
                QJsonArray { QJsonObject { { QStringLiteral("id"), QStringLiteral("low") } }, QJsonObject { { QStringLiteral("id"), QStringLiteral("high") } },
                    QJsonObject { { QStringLiteral("id"), QStringLiteral("max") } } } },
            { QStringLiteral("choice"), choice } }));
        QCOMPARE(start(QStringLiteral("claude")), starts(QStringLiteral("work"), QStringLiteral("opus"), QString()));
        QCOMPARE(plan(QStringLiteral("claude")).value(QStringLiteral("model")).toMap().value(QStringLiteral("label")).toString(), QStringLiteral("Opus"));
        QCOMPARE(plan(QStringLiteral("claude")).value(QStringLiteral("efforts")).toList().size(), 2);

        QSignalSpy plans(&launcher, &kodosi::AgentLauncherModel::plansChanged);
        launcher.choose(QStringLiteral("effort"), QStringLiteral("claude"), QStringLiteral("high"));
        launcher.choose(QStringLiteral("account"), QStringLiteral("claude"), QStringLiteral("default"));
        QCOMPARE(plans.size(), 2);
        QCOMPARE(start(QStringLiteral("claude")), starts(QStringLiteral("default"), QStringLiteral("opus"), QStringLiteral("high")));
        QCOMPARE(commands.values.last().value(QStringLiteral("change")).toObject(),
            QJsonObject({ { QStringLiteral("type"), QStringLiteral("pick") }, { QStringLiteral("agent"), QStringLiteral("claude") },
                { QStringLiteral("account"), QStringLiteral("default") }, { QStringLiteral("model"), QStringLiteral("opus") },
                { QStringLiteral("effort"), QStringLiteral("high") } }));
    }
    void usageAsksOnceAtATimeAndKeepsTheHeadlineOfEachAccount()
    {
        ProviderCommands commands;
        kodosi::AgentLauncherModel launcher(commands);
        QSignalSpy changed(&launcher, &kodosi::AgentLauncherModel::usageChanged);
        launcher.refreshUsage();
        launcher.refreshUsage();
        QCOMPARE(commands.values.size(), 1);
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("provider.usage"));
        const auto limit = [](QJsonValue hours, int used, const QString& level, bool headline) {
            return QJsonObject { { QStringLiteral("hours"), hours }, { QStringLiteral("scope"), QJsonValue() },
                { QStringLiteral("used"), used }, { QStringLiteral("level"), level }, { QStringLiteral("headline"), headline } };
        };
        const QJsonObject work { { QStringLiteral("agent"), QStringLiteral("claude") }, { QStringLiteral("account"), QStringLiteral("work") },
            { QStringLiteral("readAtMs"), qint64(1792716922000) },
            { QStringLiteral("limits"), QJsonArray { limit(5, 42, QStringLiteral("normal"), false), limit(168, 85, QStringLiteral("near"), true) } } };
        const QJsonObject copilot { { QStringLiteral("agent"), QStringLiteral("copilot") }, { QStringLiteral("account"), QStringLiteral("default") },
            { QStringLiteral("readAtMs"), qint64(1792716922000) }, { QStringLiteral("limits"), QJsonArray { limit(QJsonValue(), 59, QStringLiteral("normal"), true) } } };
        QVERIFY(!launcher.usageRead());
        launcher.apply(commands.reply({ { QStringLiteral("accounts"), QJsonArray { work, copilot } } }));
        QVERIFY(launcher.usageRead());
        QCOMPARE(changed.size(), 1);
        const auto headline = [&](const QString& key) {
            return launcher.usage().value(key).toMap().value(QStringLiteral("headline")).toMap();
        };
        QCOMPARE(headline(QStringLiteral("claude/work")).value(QStringLiteral("used")).toInt(), 85);
        QCOMPARE(headline(QStringLiteral("copilot/default")).value(QStringLiteral("level")).toString(), QStringLiteral("normal"));

        const auto limits = launcher.usage().value(QStringLiteral("claude/work")).toMap().value(QStringLiteral("limits")).toList();
        QCOMPARE(limits.first().toMap().value(QStringLiteral("window")).toString(), QStringLiteral("5h"));
        QCOMPARE(limits.last().toMap().value(QStringLiteral("spoken")).toString(), QStringLiteral("This week: 85% used"));
        QCOMPARE(headline(QStringLiteral("copilot/default")).value(QStringLiteral("window")).toString(), QStringLiteral("month"));

        launcher.refreshUsage();
        QCOMPARE(commands.values.size(), 2);
        launcher.apply(commands.reply({ { QStringLiteral("accounts"), QJsonArray { work, copilot } } }));
        QCOMPARE(changed.size(), 1);

        QVERIFY(launcher.usageSections().isEmpty());
        QVERIFY(!launcher.usageAvailable());
        const auto account = [](const QString& name, const QString& identity) {
            return QJsonObject { { QStringLiteral("name"), name }, { QStringLiteral("command"), QStringLiteral("claude") },
                { QStringLiteral("identity"), identity.isEmpty() ? QJsonValue() : QJsonValue(identity) } };
        };
        const QJsonObject claude { { QStringLiteral("agent"), QStringLiteral("claude") }, { QStringLiteral("program"), QStringLiteral("claude") },
            { QStringLiteral("installed"), true }, { QStringLiteral("install"), QString() }, { QStringLiteral("addsAccounts"), true },
            { QStringLiteral("reportsUsage"), true },
            { QStringLiteral("accounts"), QJsonArray { account(QStringLiteral("default"), QStringLiteral("ada@example.com")),
                account(QStringLiteral("claude-work"), QStringLiteral("ada@work.example")),
                account(QStringLiteral("personal"), QString()) } } };
        launcher.refresh(true);
        launcher.apply(commands.reply({ { QStringLiteral("agents"), QJsonArray { claude } } }));
        QVERIFY(launcher.usageAvailable());
        auto renamed = work;
        renamed.insert(QStringLiteral("account"), QStringLiteral("claude-work"));
        auto signedOut = work;
        signedOut.insert(QStringLiteral("account"), QStringLiteral("personal"));
        signedOut.insert(QStringLiteral("limits"), QJsonArray { limit(5, 97, QStringLiteral("near"), true) });
        launcher.refreshUsage();
        launcher.apply(commands.reply({ { QStringLiteral("accounts"), QJsonArray { renamed, signedOut } } }));
        QCOMPARE(launcher.usageNearest().toMap().value(QStringLiteral("used")).toInt(), 85);
        const auto sections = launcher.usageSections();
        QCOMPARE(sections.size(), 1);
        const auto rows = sections.first().toMap().value(QStringLiteral("rows")).toList();
        QCOMPARE(rows.size(), 1);
        QCOMPARE(rows.first().toMap().value(QStringLiteral("title")).toString(), QStringLiteral("Work"));
        QCOMPARE(sections.first().toMap().value(QStringLiteral("windows")).toList().size(), 2);
    }
    void aLauncherForAnotherComputerAsksThatComputerAndKeepsItsAnswers()
    {
        ProviderCommands commands;
        kodosi::AgentLauncherModel launcher(commands);
        auto& remote = *launcher.remote();
        remote.setComputer(QStringLiteral("laptop"));
        remote.listFolders();
        QCOMPARE(commands.values.size(), 2);
        for (const auto& command : std::as_const(commands.values))
            QCOMPARE(command.value(QStringLiteral("computer")).toString(), QStringLiteral("laptop"));
        const auto places = commands.values.last();
        QCOMPARE(places.value(QStringLiteral("type")).toString(), QStringLiteral("folders.list"));
        QVERIFY(!places.contains(QStringLiteral("path")));
        launcher.refresh(true);
        QVERIFY(!commands.values.last().contains(QStringLiteral("computer")));

        launcher.apply(commands.reply({ { QStringLiteral("agents"), QJsonArray { QJsonObject { { QStringLiteral("agent"), QStringLiteral("codex") } } } } }, 0));
        QCOMPARE(remote.agents().size(), 1);
        QVERIFY(launcher.agents().isEmpty());
        const auto folders = [](const QJsonObject& command, const QString& path, const QStringList& names) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("folders.result") },
                { QStringLiteral("requestId"), command.value(QStringLiteral("requestId")) },
                { QStringLiteral("operation"), QStringLiteral("folders.list") },
                { QStringLiteral("folders"),
                    QJsonObject { { QStringLiteral("path"), path }, { QStringLiteral("parent"), QStringLiteral("/home") },
                        { QStringLiteral("home"), QStringLiteral("/home/ada") }, { QStringLiteral("folders"), QJsonArray::fromStringList(names) },
                        { QStringLiteral("recent"), QJsonArray { QStringLiteral("/home/ada/kodosi") } } } } };
        };
        launcher.apply(folders(places, QStringLiteral("/home/ada"), { QStringLiteral("kodosi") }));
        QCOMPARE(remote.places().value(QStringLiteral("recent")).toStringList(), QStringList { QStringLiteral("/home/ada/kodosi") });
        QVERIFY(remote.listing().isEmpty());
        QVERIFY(launcher.places().isEmpty());

        remote.listFolders(QStringLiteral("/home/ada/kodosi"), true);
        const auto browse = commands.values.last();
        QCOMPARE(browse.value(QStringLiteral("path")).toString(), QStringLiteral("/home/ada/kodosi"));
        launcher.apply(folders(browse, QStringLiteral("/home/ada/kodosi"), { QStringLiteral("runtime") }));
        QCOMPARE(remote.listing().value(QStringLiteral("folders")).toStringList(), QStringList { QStringLiteral("runtime") });
        QCOMPARE(remote.places().value(QStringLiteral("path")).toString(), QStringLiteral("/home/ada"));

        remote.listFolders(QStringLiteral("/home/ada/kodosi/gone"), true);
        launcher.apply({ { QStringLiteral("type"), QStringLiteral("folders.error") },
            { QStringLiteral("requestId"), commands.values.last().value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), QStringLiteral("folders.list") }, { QStringLiteral("message"), QStringLiteral("This folder is not there.") } });
        QCOMPARE(remote.folderError(), QStringLiteral("This folder is not there."));
        QCOMPARE(remote.listing().value(QStringLiteral("path")).toString(), QStringLiteral("/home/ada/kodosi"));
        remote.closeFolders();
        QVERIFY(remote.listing().isEmpty() && remote.folderError().isEmpty());

        remote.listFolders(QStringLiteral("/home/ada"), true);
        const auto older = commands.values.last();
        remote.listFolders(QStringLiteral("/home"), true);
        launcher.apply(folders(commands.values.last(), QStringLiteral("/home"), { QStringLiteral("ada") }));
        launcher.apply(folders(older, QStringLiteral("/home/ada"), { QStringLiteral("kodosi") }));
        QCOMPARE(remote.listing().value(QStringLiteral("path")).toString(), QStringLiteral("/home"));
        remote.listFolders(QStringLiteral("/home/ada"), true);
        const auto late = commands.values.last();
        remote.closeFolders();
        launcher.apply(folders(late, QStringLiteral("/home/ada"), {}));
        QVERIFY(remote.listing().isEmpty());

        const auto sent = commands.values.size();
        remote.setComputer({});
        QVERIFY(remote.places().isEmpty() && remote.agents().isEmpty());
        QCOMPARE(commands.values.size(), sent);
    }
};
QTEST_GUILESS_MAIN(ProviderModelsTest)
#include "tst_provider_models.moc"
