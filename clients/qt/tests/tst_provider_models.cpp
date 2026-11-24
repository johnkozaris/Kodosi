#include "SessionFixture.hpp"
#include "presentation/AgentLauncherModel.hpp"
#include <QJsonDocument>
#include <QSignalSpy>
#include <QtTest/QTest>

class ProviderCommands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    QJsonObject reply(QJsonObject result) const
    {
        const auto command = values.last();
        return { { QStringLiteral("type"), QStringLiteral("provider.result") },
            { QStringLiteral("requestId"), command.value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), command.value(QStringLiteral("type")) },
            { QStringLiteral("result"), result } };
    }
};

class ProviderModelsTest final : public QObject {
    Q_OBJECT
private slots:
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
            { QStringLiteral("addsAccounts"), true },
            { QStringLiteral("permissions"), QJsonArray { QStringLiteral("ask"), QStringLiteral("auto"), QStringLiteral("unrestricted") } },
            { QStringLiteral("outputs"), QJsonArray { QStringLiteral("quiet"), QStringLiteral("standard"), QStringLiteral("detailed") } },
            { QStringLiteral("defaultPermissions"), QStringLiteral("unrestricted") }, { QStringLiteral("defaultOutput"), QStringLiteral("quiet") },
            { QStringLiteral("fast"), true } };
        const QJsonObject codex { { QStringLiteral("agent"), QStringLiteral("codex") },
            { QStringLiteral("program"), QStringLiteral("codex") }, { QStringLiteral("installed"), false },
            { QStringLiteral("accounts"), QJsonArray {} }, { QStringLiteral("install"), QStringLiteral("npm install -g @openai/codex") },
            { QStringLiteral("addsAccounts"), true }, { QStringLiteral("defaultPermissions"), QStringLiteral("unrestricted") },
            { QStringLiteral("defaultOutput"), QStringLiteral("quiet") } };
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
                { QStringLiteral("model"), model }, { QStringLiteral("effort"), effort },
                { QStringLiteral("permissions"), QStringLiteral("unrestricted") }, { QStringLiteral("output"), QStringLiteral("quiet") },
                { QStringLiteral("fast"), false } };
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
                { QStringLiteral("effort"), QStringLiteral("high") }, { QStringLiteral("permissions"), QStringLiteral("unrestricted") },
                { QStringLiteral("output"), QStringLiteral("quiet") }, { QStringLiteral("fast"), false },
                { QStringLiteral("chat"), false } }));

        launcher.choose(QStringLiteral("permissions"), QStringLiteral("claude"), QStringLiteral("ask"));
        launcher.choose(QStringLiteral("output"), QStringLiteral("claude"), QStringLiteral("standard"));
        QCOMPARE(start(QStringLiteral("claude")).value(QStringLiteral("permissions")).toString(), QStringLiteral("ask"));
        QCOMPARE(start(QStringLiteral("claude")).value(QStringLiteral("output")).toString(), QStringLiteral("standard"));
        const auto change = commands.values.last().value(QStringLiteral("change")).toObject();
        QCOMPARE(change.value(QStringLiteral("permissions")).toString(), QStringLiteral("ask"));
        QCOMPARE(change.value(QStringLiteral("output")).toString(), QStringLiteral("standard"));

        launcher.choose(QStringLiteral("fast"), QStringLiteral("claude"), true);
        QVERIFY(start(QStringLiteral("claude")).value(QStringLiteral("fast")).toBool());
        QVERIFY(commands.values.last().value(QStringLiteral("change")).toObject().value(QStringLiteral("fast")).toBool());
    }
};
QTEST_GUILESS_MAIN(ProviderModelsTest)
#include "tst_provider_models.moc"
