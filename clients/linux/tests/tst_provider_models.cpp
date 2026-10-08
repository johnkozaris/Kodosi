#include "SessionFixture.hpp"
#include "presentation/ConversationHistoryModel.hpp"
#include "presentation/ProviderFilesModel.hpp"
#include <QJsonDocument>
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
    QJsonObject reply(QJsonObject result, int index = -1) const
    {
        const auto command = values.at(index < 0 ? values.size() - 1 : index);
        return { { QStringLiteral("type"), QStringLiteral("provider.reply") },
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
        tools.open(kodosi::ConversationHistoryModel::Claude, QStringLiteral("/repo"));
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
        tools.open(kodosi::ConversationHistoryModel::Copilot, QStringLiteral("/other"));
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
        tools.open(kodosi::ConversationHistoryModel::Claude, QStringLiteral("/repo"));
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
        tools.open(kodosi::ConversationHistoryModel::Claude, {});
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
        tools.open(kodosi::ConversationHistoryModel::Claude, QStringLiteral("/repo"));
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
        tools.open(kodosi::ConversationHistoryModel::Claude, QStringLiteral("/repo"));
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
        files.inspect(kodosi::ProviderFilesModel::Claude, QStringLiteral("/repo"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(),
            QStringLiteral("provider.inspect"));
        QVERIFY(!commands.values.last().contains(QStringLiteral("content")));
        files.apply(commands.reply({ { QStringLiteral("provider"), QStringLiteral("claude") },
            { QStringLiteral("files"), QJsonArray {} } }));
        QVERIFY(!files.busy());
    }
};
QTEST_GUILESS_MAIN(ProviderModelsTest)
#include "tst_provider_models.moc"
