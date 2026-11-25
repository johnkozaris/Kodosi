#include "SessionFixture.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/ChatProse.hpp"
#include "presentation/ChatWords.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/TerminalChat.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QTemporaryDir>
#include <QtTest/QTest>

namespace {
class Commands final : public kodosi::CommandDispatcher, public kodosi::PipeRouter {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    void route(const QString&, QObject&, Read) override { }
    void unroute(const QString&) override { }
    [[nodiscard]] QList<QJsonObject> of(const QString& type) const
    {
        QList<QJsonObject> found;
        for (const auto& value : values) {
            if (value.value(QStringLiteral("type")).toString() == type)
                found.append(value);
        }
        return found;
    }
    // What the app sent on the chat's pipe, as the host reads it.
    [[nodiscard]] QList<QJsonObject> requests() const
    {
        QList<QJsonObject> found;
        for (const auto& value : of(QStringLiteral("document.send")))
            found.append(QJsonDocument::fromJson(QByteArray::fromBase64(value.value(QStringLiteral("data")).toString().toLatin1())).object());
        return found;
    }
};

QJsonObject item(const QString& id, QJsonObject body, qint64 at = 1795226745000)
{
    return { { QStringLiteral("id"), id }, { QStringLiteral("at"), at }, { QStringLiteral("body"), body } };
}

QJsonObject person(const QString& id, const QString& text)
{
    return item(id, { { QStringLiteral("kind"), QStringLiteral("person") }, { QStringLiteral("text"), text } });
}

QJsonObject words(const QString& id, const QString& text, bool live = false)
{
    return item(id, { { QStringLiteral("kind"), QStringLiteral("text") }, { QStringLiteral("text"), text }, { QStringLiteral("live"), live } });
}

QJsonObject tool(const QString& id, const QString& kind, const QString& state, const QString& target)
{
    return item(id, { { QStringLiteral("kind"), QStringLiteral("tool") },
                        { QStringLiteral("tool"), QJsonObject { { QStringLiteral("name"), kind }, { QStringLiteral("kind"), kind },
                                                      { QStringLiteral("state"), state }, { QStringLiteral("target"), target } } } });
}

QJsonObject helper(const QString& id, const QString& state, const QString& task, const QJsonObject& detail = {})
{
    QJsonObject call { { QStringLiteral("name"), QStringLiteral("Task") }, { QStringLiteral("kind"), QStringLiteral("agent") },
        { QStringLiteral("state"), state }, { QStringLiteral("target"), task } };
    if (!detail.isEmpty())
        call.insert(QStringLiteral("detail"), detail);
    return item(id, { { QStringLiteral("kind"), QStringLiteral("tool") }, { QStringLiteral("tool"), call } });
}

QJsonObject turnEnd(const QString& id, const QString& end, qint64 ms)
{
    return item(id, { { QStringLiteral("kind"), QStringLiteral("turn") }, { QStringLiteral("end"), end }, { QStringLiteral("ms"), ms } });
}

QJsonObject permission(const QString& id, const QString& command)
{
    return { { QStringLiteral("id"), id },
        { QStringLiteral("body"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("permission") },
                                      { QStringLiteral("tool"), QJsonObject { { QStringLiteral("name"), QStringLiteral("Bash") },
                                                                    { QStringLiteral("kind"), QStringLiteral("command") },
                                                                    { QStringLiteral("state"), QStringLiteral("asking") },
                                                                    { QStringLiteral("target"), command } } } } } };
}

QByteArray snapshot(int revision, const QString& state, QJsonArray items, QJsonArray asks = {})
{
    return QJsonDocument(QJsonObject { { QStringLiteral("snapshot"),
                                QJsonObject { { QStringLiteral("revision"), revision }, { QStringLiteral("state"), state },
                                    { QStringLiteral("facts"), QJsonObject {} }, { QStringLiteral("items"), items },
                                    { QStringLiteral("asks"), asks } } } })
        .toJson(QJsonDocument::Compact);
}

QByteArray update(int revision, QJsonArray changes)
{
    return QJsonDocument(QJsonObject { { QStringLiteral("update"),
                                QJsonObject { { QStringLiteral("revision"), revision }, { QStringLiteral("changes"), changes } } } })
        .toJson(QJsonDocument::Compact);
}

QStringList kinds(const QVariantList& lines)
{
    QStringList found;
    for (const auto& line : lines)
        found.append(line.toMap().value(QStringLiteral("kind")).toString());
    return found;
}

QVariantMap line(const QVariantList& lines, const QString& kind)
{
    for (const auto& line : lines) {
        if (line.toMap().value(QStringLiteral("kind")).toString() == kind)
            return line.toMap();
    }
    return {};
}
}

class TerminalChatTest final : public QObject {
    Q_OBJECT

private slots:
    void anAgentsWordsBecomeRichTextButNeverMarkup()
    {
        QCOMPARE(kodosi::ChatProse::html(u"<b>hi</b> & \"you\""), QStringLiteral("&lt;b&gt;hi&lt;/b&gt; &amp; &quot;you&quot;"));
        QCOMPARE(kodosi::ChatProse::html(u"**bold** and *soft* and `a*b*c`"),
            QStringLiteral("<b>bold</b> and <i>soft</i> and <code>a*b*c</code>"));
        QCOMPARE(kodosi::ChatProse::html(u"price_with_tax stays"), QStringLiteral("price_with_tax stays"));
        QCOMPARE(kodosi::ChatProse::html(u"[docs](https://kodosi.com/a?b=1&c=2)"),
            QStringLiteral("<a href=\"https://kodosi.com/a?b=1&amp;c=2\">docs</a>"));
        QCOMPARE(kodosi::ChatProse::html(u"[run](javascript:alert(1)) and http://plain.example"),
            QStringLiteral("[run](javascript:alert(1)) and http://plain.example"));
        QCOMPARE(kodosi::ChatProse::html(u"see https://kodosi.com."), QStringLiteral("see <a href=\"https://kodosi.com\">https://kodosi.com</a>."));
    }

    void anAnswerHasItsBlocksAndAFenceThatIsStillOpenHoldsTheRest()
    {
        const auto blocks = kodosi::ChatProse::blocks(QStringLiteral(
            "## Prices\n\nIn the cart:\n- mug\n  * bowl\n| Item | Price \\| EUR |\n|:--|--:|\n| Mug | 24 |\n| Bowl |\nAfter.\n```ts\nconst a = 1\n"));
        QCOMPARE(kinds(blocks), QStringList({ QStringLiteral("heading"), QStringLiteral("lines"), QStringLiteral("table"),
                                    QStringLiteral("lines"), QStringLiteral("code") }));
        QCOMPARE(blocks.at(0).toMap().value(QStringLiteral("html")).toString(), QStringLiteral("Prices"));
        QVERIFY(blocks.at(1).toMap().value(QStringLiteral("html")).toString().contains(QStringLiteral("• mug\n  • bowl")));
        const auto table = blocks.at(2).toMap();
        QCOMPARE(table.value(QStringLiteral("header")).toList(), QVariantList({ QStringLiteral("Item"), QStringLiteral("Price | EUR") }));
        QCOMPARE(table.value(QStringLiteral("sides")).toList(), QVariantList({ QStringLiteral("left"), QStringLiteral("right") }));
        QCOMPARE(table.value(QStringLiteral("rows")).toList(),
            QVariantList({ QVariantList({ QStringLiteral("Mug"), QStringLiteral("24") }), QVariantList({ QStringLiteral("Bowl"), QString {} }) }));
        QCOMPARE(blocks.at(4).toMap().value(QStringLiteral("text")).toString(), QStringLiteral("const a = 1\n"));
    }

    void aModelIsNamedAsTheLauncherKnowsIt()
    {
        const QVariantMap models { { QStringLiteral("models"),
            QVariantList { QVariantMap { { QStringLiteral("id"), QStringLiteral("sonnet") }, { QStringLiteral("label"), QStringLiteral("Sonnet") } },
                QVariantMap { { QStringLiteral("id"), QStringLiteral("gpt-5.5") }, { QStringLiteral("label"), QStringLiteral("GPT-5.5") } } } } };
        QCOMPARE(kodosi::ChatWords::modelName(QStringLiteral("claude-sonnet-5-5"), models), QStringLiteral("Sonnet 5.5"));
        QCOMPARE(kodosi::ChatWords::modelName(QStringLiteral("claude-sonnet-4-5-20250929[1m]"), models), QStringLiteral("Sonnet 4.5"));
        QCOMPARE(kodosi::ChatWords::modelName(QStringLiteral("gpt-5.5"), models), QStringLiteral("GPT-5.5"));
        QCOMPARE(kodosi::ChatWords::modelName(QStringLiteral("mai-code-2"), models), QStringLiteral("mai-code-2"));
    }

    void theChatOpensItsPipeWhileAViewWatchesAndCountsChangesFromTheSnapshot()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChats chats(documents, sessions);
        auto* chat = chats.chat(test::id(1));
        QCOMPARE(chats.chat(test::id(1)), chat);
        chat->watch();
        chat->watch();
        QCOMPARE(commands.of(QStringLiteral("document.open")).size(), 1);
        const auto pipe = commands.of(QStringLiteral("document.open")).first().value(QStringLiteral("pipe")).toString();
        const auto frame = [&](const QByteArray& bytes) {
            static int sequence = 0;
            documents.apply({ { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
                { QStringLiteral("sequence"), sequence++ }, { QStringLiteral("more"), false },
                { QStringLiteral("data"), QString::fromLatin1(bytes.toBase64()) } });
        };
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } });
        frame(update(1, { QJsonObject { { QStringLiteral("change"), QStringLiteral("reset") } } }));
        QVERIFY(!chat->loaded());

        frame(snapshot(4, QStringLiteral("working"), { person(QStringLiteral("p1"), QStringLiteral("Hi")), words(QStringLiteral("t1"), QStringLiteral("Hel"), true) }));
        QVERIFY(chat->loaded());
        QVERIFY(chat->canWrite());
        frame(update(5, { QJsonObject { { QStringLiteral("change"), QStringLiteral("text") }, { QStringLiteral("id"), QStringLiteral("t1") }, { QStringLiteral("text"), QStringLiteral("lo") } } }));
        QCOMPARE(line(chat->lines(), QStringLiteral("answer")).value(QStringLiteral("text")).toString(), QStringLiteral("Hello"));
        QVERIFY(line(chat->lines(), QStringLiteral("answer")).value(QStringLiteral("live")).toBool());

        const auto opened = commands.of(QStringLiteral("document.open")).size();
        frame(update(7, { QJsonObject { { QStringLiteral("change"), QStringLiteral("reset") } } }));
        QCOMPARE(commands.of(QStringLiteral("document.close")).size(), 1);
        QCOMPARE(commands.of(QStringLiteral("document.open")).size(), opened + 1);
        QVERIFY(!chat->canWrite());
        QCOMPARE(line(chat->lines(), QStringLiteral("answer")).value(QStringLiteral("text")).toString(), QStringLiteral("Hello"));

        chat->unwatch();
        QCOMPARE(commands.of(QStringLiteral("document.close")).size(), 1);
        chat->unwatch();
        QCOMPARE(commands.of(QStringLiteral("document.close")).size(), 2);
    }

    void aTurnShowsTheAgentsWordsAndFoldsTheStepsBetweenThem()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChat chat(documents, test::id(1));
        chat.watch();
        chat.pipeOpened();
        chat.receive(snapshot(1, QStringLiteral("idle"),
            { person(QStringLiteral("p1"), QStringLiteral("Fix it")), words(QStringLiteral("t1"), QStringLiteral("Looking.")),
                tool(QStringLiteral("c1"), QStringLiteral("read"), QStringLiteral("done"), QStringLiteral("cart.ts")),
                tool(QStringLiteral("c2"), QStringLiteral("command"), QStringLiteral("denied"), QStringLiteral("rm -rf build")),
                words(QStringLiteral("t2"), QStringLiteral("Done.")), turnEnd(QStringLiteral("e1"), QStringLiteral("done"), 45000),
                person(QStringLiteral("p2"), QStringLiteral("And?")), words(QStringLiteral("t3"), QStringLiteral("Nothing else.")),
                turnEnd(QStringLiteral("e2"), QStringLiteral("done"), 900) }));
        const auto lines = chat.lines();
        QCOMPARE(kinds(lines), QStringList({ QStringLiteral("person"), QStringLiteral("answer"), QStringLiteral("work"),
                                   QStringLiteral("answer"), QStringLiteral("end"), QStringLiteral("person"), QStringLiteral("answer") }));
        QVERIFY(lines.at(1).toMap().value(QStringLiteral("lead")).toBool());
        QCOMPARE(lines.at(1).toMap().value(QStringLiteral("text")).toString(), QStringLiteral("Looking."));
        const auto work = lines.at(2).toMap();
        QVERIFY(!work.value(QStringLiteral("lead")).toBool());
        QCOMPARE(work.value(QStringLiteral("steps")).toList().size(), 2);
        QVERIFY(!work.value(QStringLiteral("done")).toBool());
        QVERIFY(!lines.at(3).toMap().value(QStringLiteral("lead")).toBool());
        QCOMPARE(lines.at(3).toMap().value(QStringLiteral("text")).toString(), QStringLiteral("Done."));
        QVERIFY(!lines.at(4).toMap().value(QStringLiteral("worked")).toString().isEmpty());
        QVERIFY(lines.at(6).toMap().value(QStringLiteral("lead")).toBool());
        QVERIFY(!chat.empty());
    }

    void whileTheAgentWorksAndAsksNothingTheEndOfItsTurnSaysWhatItDoes()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChat chat(documents, test::id(1));
        chat.watch();
        chat.receive(snapshot(1, QStringLiteral("working"),
            { person(QStringLiteral("p1"), QStringLiteral("Test")),
                tool(QStringLiteral("c1"), QStringLiteral("command"), QStringLiteral("running"), QStringLiteral("npm test")) }));
        QCOMPARE(kinds(chat.lines()), QStringList({ QStringLiteral("person"), QStringLiteral("work"), QStringLiteral("now") }));
        QVERIFY(chat.lines().at(1).toMap().value(QStringLiteral("working")).toBool());
        chat.receive(update(2, { QJsonObject { { QStringLiteral("change"), QStringLiteral("asks") },
                                    { QStringLiteral("asks"), QJsonArray { permission(QStringLiteral("a1"), QStringLiteral("npm test")) } } } }));
        QCOMPARE(kinds(chat.lines()), QStringList({ QStringLiteral("person"), QStringLiteral("work") }));
        QCOMPARE(chat.ask().value(QStringLiteral("title")).toString(), QStringLiteral("npm test"));
    }

    void onlyTheHelpersThatRunShowInTheOrderThatTheAgentStartedThem()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChat chat(documents, test::id(1));
        chat.watch();
        const QJsonObject explore { { QStringLiteral("kind"), QStringLiteral("agent") }, { QStringLiteral("name"), QStringLiteral("Explore") },
            { QStringLiteral("steps"), 4 },
            { QStringLiteral("doing"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("read") }, { QStringLiteral("target"), QStringLiteral("src/tax.ts") } } },
            { QStringLiteral("said"), QStringLiteral("Found the rate") } };
        chat.receive(snapshot(1, QStringLiteral("working"),
            { person(QStringLiteral("p1"), QStringLiteral("Fix the tax")),
                helper(QStringLiteral("h1"), QStringLiteral("running"), QStringLiteral("Find the tax code"), explore),
                helper(QStringLiteral("h2"), QStringLiteral("done"), QStringLiteral("Read the docs"), explore),
                helper(QStringLiteral("h3"), QStringLiteral("preparing"), QStringLiteral("Check the cart")),
                tool(QStringLiteral("c1"), QStringLiteral("command"), QStringLiteral("running"), QStringLiteral("npm test")),
                helper(QStringLiteral("h4"), QStringLiteral("running"), QStringLiteral("Check the tests")) }));
        auto helpers = chat.helpers();
        QCOMPARE(helpers.size(), 2);
        const auto explorer = helpers.at(0).toMap();
        QCOMPARE(explorer.value(QStringLiteral("id")).toString(), QStringLiteral("h1"));
        QCOMPARE(explorer.value(QStringLiteral("kind")).toString(), QStringLiteral("Explore"));
        QCOMPARE(explorer.value(QStringLiteral("task")).toString(), QStringLiteral("Find the tax code"));
        QCOMPARE(explorer.value(QStringLiteral("steps")).toInt(), 4);
        QCOMPARE(explorer.value(QStringLiteral("said")).toString(), QStringLiteral("Found the rate"));
        const auto doing = explorer.value(QStringLiteral("doing")).toMap();
        QCOMPARE(doing.value(QStringLiteral("target")).toString(), QStringLiteral("src/tax.ts"));
        QCOMPARE(doing.value(QStringLiteral("icon")).toString(), kodosi::ChatWords::icon(QStringLiteral("read")));
        const auto bare = helpers.at(1).toMap();
        QCOMPARE(bare.value(QStringLiteral("id")).toString(), QStringLiteral("h4"));
        QCOMPARE(bare.value(QStringLiteral("kind")).toString(), QString {});
        QCOMPARE(bare.value(QStringLiteral("steps")).toInt(), 0);
        QVERIFY(!bare.contains(QStringLiteral("doing")));

        chat.receive(update(2, { QJsonObject { { QStringLiteral("change"), QStringLiteral("item") },
                                    { QStringLiteral("item"), helper(QStringLiteral("h1"), QStringLiteral("done"), QStringLiteral("Find the tax code"), explore) } } }));
        helpers = chat.helpers();
        QCOMPARE(helpers.size(), 1);
        QCOMPARE(helpers.at(0).toMap().value(QStringLiteral("id")).toString(), QStringLiteral("h4"));
    }

    void wordsTypedWhileAQuestionWaitsAnswerItAndEachQuestionTakesOneAnswer()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChat chat(documents, test::id(1));
        chat.watch();
        chat.receive(snapshot(1, QStringLiteral("asking"), {}, { permission(QStringLiteral("a1"), QStringLiteral("npm test")) }));
        QVERIFY(chat.asking());
        chat.setDraft(QStringLiteral("  Only the cart tests  "));
        QVERIFY(chat.submit());
        QCOMPARE(chat.draft(), QString {});
        chat.answer(QStringLiteral("a1"), { { QStringLiteral("kind"), QStringLiteral("allow") } });
        const auto sent = commands.requests();
        QCOMPARE(sent.size(), 1);
        QCOMPARE(sent.first().value(QStringLiteral("do")).toString(), QStringLiteral("answer"));
        QCOMPARE(sent.first().value(QStringLiteral("answer")).toObject(),
            QJsonObject({ { QStringLiteral("kind"), QStringLiteral("deny") }, { QStringLiteral("text"), QStringLiteral("Only the cart tests") } }));
        QVERIFY(!chat.asking());

        chat.receive(update(2, { QJsonObject { { QStringLiteral("change"), QStringLiteral("asks") }, { QStringLiteral("asks"), QJsonArray {} } } }));
        chat.setDraft(QStringLiteral("Next"));
        QVERIFY(chat.submit());
        QCOMPARE(commands.requests().last(), QJsonObject({ { QStringLiteral("do"), QStringLiteral("send") }, { QStringLiteral("text"), QStringLiteral("Next") } }));
    }

    void aPickedFaceHoldsUntilTheAgentReportsAnother()
    {
        QTemporaryDir folder;
        auto settings = std::make_unique<QSettings>(folder.filePath(QStringLiteral("preferences.ini")), QSettings::IniFormat);
        kodosi::SessionCatalogModel sessions;
        kodosi::DesktopStateModel desktop(std::move(settings));
        desktop.attachSessionCatalog(&sessions);
        auto entry = test::session(1);
        entry.insert(QStringLiteral("face"), QStringLiteral("terminal"));
        sessions.apply(test::snapshot({ entry }));
        desktop.pickFace(test::id(1), QStringLiteral("chat"));
        sessions.apply(test::snapshot({ entry }));
        QCOMPARE(desktop.faces().value(test::id(1)).toString(), QStringLiteral("chat"));
        entry.insert(QStringLiteral("face"), QStringLiteral("chat"));
        sessions.apply(test::snapshot({ entry }));
        QVERIFY(!desktop.faces().contains(test::id(1)));
        desktop.pickFace(test::id(1), QStringLiteral("terminal"));
        sessions.apply(test::snapshot({}));
        QVERIFY(!desktop.faces().contains(test::id(1)));
    }

    void aTerminalStartedInItsChatShowsItUntilItsAgentReportsWhereItWorks()
    {
        QTemporaryDir folder;
        auto settings = std::make_unique<QSettings>(folder.filePath(QStringLiteral("preferences.ini")), QSettings::IniFormat);
        kodosi::SessionCatalogModel sessions;
        kodosi::DesktopStateModel desktop(std::move(settings));
        desktop.attachSessionCatalog(&sessions);
        desktop.pickFace(test::id(2), QStringLiteral("chat"));
        sessions.apply(test::snapshot({}));
        QCOMPARE(desktop.faces().value(test::id(2)).toString(), QStringLiteral("chat"));
        auto entry = test::session(2);
        sessions.apply(test::snapshot({ entry }));
        QCOMPARE(desktop.faces().value(test::id(2)).toString(), QStringLiteral("chat"));
        entry.insert(QStringLiteral("face"), QStringLiteral("terminal"));
        sessions.apply(test::snapshot({ entry }));
        QVERIFY(!desktop.faces().contains(test::id(2)));
    }
};

QTEST_MAIN(TerminalChatTest)
#include "tst_terminal_chat.moc"
