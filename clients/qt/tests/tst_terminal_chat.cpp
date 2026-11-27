#include "SessionFixture.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/ChatProse.hpp"
#include "presentation/TerminalChat.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QtTest/QTest>

namespace {
class Commands final : public kodosi::CommandDispatcher, public kodosi::VideoWatcher {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    void watch(const QString&, std::shared_ptr<kodosi::VideoSink>) override { }
    void unwatch(const QString&) override { }
    [[nodiscard]] QList<QJsonObject> of(const QString& type) const
    {
        QList<QJsonObject> found;
        for (const auto& value : values) {
            if (value.value(QStringLiteral("type")).toString() == type)
                found.append(value);
        }
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

QByteArray snapshot(int revision, const QString& state, QJsonArray items)
{
    return QJsonDocument(QJsonObject { { QStringLiteral("snapshot"),
                                QJsonObject { { QStringLiteral("revision"), revision }, { QStringLiteral("state"), state },
                                    { QStringLiteral("facts"), QJsonObject {} }, { QStringLiteral("items"), items } } } })
        .toJson(QJsonDocument::Compact);
}

QByteArray update(int revision, QJsonArray changes)
{
    return QJsonDocument(QJsonObject { { QStringLiteral("update"),
                                QJsonObject { { QStringLiteral("revision"), revision }, { QStringLiteral("changes"), changes } } } })
        .toJson(QJsonDocument::Compact);
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

    void theChatKnowsThePeersOfItsAgentAndTheAgentsThatAMessageNamesOrHandsOver()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::TerminalChats chats(documents, sessions);
        auto* chat = chats.chat(test::id(1));
        chat->watch();
        const auto pipe = commands.of(QStringLiteral("document.open")).first().value(QStringLiteral("pipe")).toString();
        int sequence = 0;
        const auto frame = [&](const QJsonObject& value) {
            documents.apply({ { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
                { QStringLiteral("sequence"), sequence++ }, { QStringLiteral("more"), false },
                { QStringLiteral("data"), QString::fromLatin1(QJsonDocument(value).toJson(QJsonDocument::Compact).toBase64()) } });
        };
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } });
        const QJsonObject elk { { QStringLiteral("name"), QStringLiteral("Timber Elk") }, { QStringLiteral("agent"), QStringLiteral("copilot") } };
        const QJsonArray peers { QJsonObject { { QStringLiteral("terminal"), test::id(2) }, { QStringLiteral("name"), QStringLiteral("Timber Elk") },
            { QStringLiteral("agent"), QStringLiteral("copilot") }, { QStringLiteral("place"), QStringLiteral("KodosiWebsite") } } };
        const auto named = item(QStringLiteral("p1"), { { QStringLiteral("kind"), QStringLiteral("person") },
                                                         { QStringLiteral("text"), QStringLiteral("Ask @Timber Elk") }, { QStringLiteral("mentions"), QJsonArray { elk } } });
        const auto handed = item(QStringLiteral("p2"), { { QStringLiteral("kind"), QStringLiteral("person") }, { QStringLiteral("text"), QString {} },
                                                          { QStringLiteral("handoff"), elk } });
        frame({ { QStringLiteral("snapshot"), QJsonObject { { QStringLiteral("revision"), 4 }, { QStringLiteral("state"), QStringLiteral("working") },
                                                    { QStringLiteral("facts"), QJsonObject {} }, { QStringLiteral("items"), QJsonArray { named, handed } },
                                                    { QStringLiteral("peers"), peers },
                                                    { QStringLiteral("queued"), QJsonArray { QJsonObject { { QStringLiteral("id"), QStringLiteral("q1") },
                                                                                    { QStringLiteral("text"), QString {} }, { QStringLiteral("handoff"), elk } } } } } } });
        QCOMPARE(chat->peers(), (QVariantList { QVariantMap { { QStringLiteral("terminal"), test::id(2) }, { QStringLiteral("name"), QStringLiteral("Timber Elk") },
                                    { QStringLiteral("agent"), QStringLiteral("copilot") }, { QStringLiteral("place"), QStringLiteral("KodosiWebsite") } } }));
        const QVariantMap shownElk { { QStringLiteral("name"), QStringLiteral("Timber Elk") }, { QStringLiteral("agent"), QStringLiteral("copilot") } };
        const auto people = chat->lines();
        QCOMPARE(people.at(0).toMap().value(QStringLiteral("mentions")), QVariant(QVariantList { shownElk }));
        QVERIFY(!people.at(0).toMap().contains(QStringLiteral("handoff")));
        QCOMPARE(people.at(1).toMap().value(QStringLiteral("handoff")), QVariant(shownElk));
        QCOMPARE(chat->queued().first().toMap().value(QStringLiteral("handoff")), QVariant(shownElk));

        frame({ { QStringLiteral("update"), QJsonObject { { QStringLiteral("revision"), 5 },
                                                  { QStringLiteral("changes"), QJsonArray { QJsonObject { { QStringLiteral("change"), QStringLiteral("reset") } } } } } } });
        QCOMPARE(chat->peers().size(), 1);
        frame({ { QStringLiteral("update"), QJsonObject { { QStringLiteral("revision"), 6 },
                                                  { QStringLiteral("changes"), QJsonArray { QJsonObject { { QStringLiteral("change"), QStringLiteral("peers") },
                                                                                   { QStringLiteral("peers"), QJsonArray {} } } } } } } });
        QVERIFY(chat->peers().isEmpty());

        chat->handOff(test::id(2));
        const auto sent = QJsonDocument::fromJson(QByteArray::fromBase64(
            commands.of(QStringLiteral("document.send")).last().value(QStringLiteral("data")).toString().toLatin1())).object();
        QCOMPARE(sent, (QJsonObject { { QStringLiteral("do"), QStringLiteral("handOff") }, { QStringLiteral("terminal"), test::id(2) } }));
    }
};

QTEST_MAIN(TerminalChatTest)
#include "tst_terminal_chat.moc"
