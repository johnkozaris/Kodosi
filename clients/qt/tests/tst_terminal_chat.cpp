#include "SessionFixture.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/ChatProse.hpp"
#include "presentation/TerminalChat.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QBuffer>
#include <QClipboard>
#include <QFile>
#include <QGuiApplication>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QMimeData>
#include <QTemporaryDir>
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

QImage shot()
{
    QImage picture(300, 200, QImage::Format_RGB32);
    picture.fill(Qt::darkCyan);
    return picture;
}

QUrl shotFile(const QTemporaryDir& folder, const QString& name, const char* format)
{
    const auto path = folder.filePath(name);
    shot().save(path, format);
    return QUrl::fromLocalFile(path);
}

// A chat whose pipe is open, with the frames that the host sends it and the requests that it sends.
struct OpenChat {
    Commands commands;
    kodosi::SessionCatalogModel sessions;
    kodosi::CanvasDocuments documents { commands, commands, sessions };
    kodosi::TerminalChats chats { documents, sessions };
    kodosi::TerminalChat* chat = chats.chat(test::id(1));
    QString pipe;
    int sequence = 0;

    OpenChat()
    {
        chat->watch();
        pipe = commands.of(QStringLiteral("document.open")).first().value(QStringLiteral("pipe")).toString();
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } });
    }
    void frame(const QJsonObject& value)
    {
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
            { QStringLiteral("sequence"), sequence++ }, { QStringLiteral("more"), false },
            { QStringLiteral("data"), QString::fromLatin1(QJsonDocument(value).toJson(QJsonDocument::Compact).toBase64()) } });
    }
    [[nodiscard]] QJsonObject sent() const
    {
        return QJsonDocument::fromJson(QByteArray::fromBase64(
            commands.of(QStringLiteral("document.send")).last().value(QStringLiteral("data")).toString().toLatin1())).object();
    }
};
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

    void aWaitingMessageSaysWhenTheAgentReadsItAndEachMessageCountsItsPictures()
    {
        OpenChat open;
        const auto waiting = [](const QString& id, const QString& given) {
            QJsonObject queued { { QStringLiteral("id"), id }, { QStringLiteral("text"), id } };
            if (!given.isEmpty())
                queued.insert(QStringLiteral("given"), given);
            return queued;
        };
        auto pictured = waiting(QStringLiteral("q1"), QStringLiteral("turn"));
        pictured.insert(QStringLiteral("pictures"), QJsonArray { QJsonObject { { QStringLiteral("name"), QStringLiteral("picture-1.png") } }, QJsonObject {} });
        const auto said = item(QStringLiteral("p1"), { { QStringLiteral("kind"), QStringLiteral("person") }, { QStringLiteral("text"), QString {} },
                                                        { QStringLiteral("pictures"), QJsonArray { QJsonObject {} } } });
        open.frame({ { QStringLiteral("snapshot"), QJsonObject { { QStringLiteral("revision"), 1 }, { QStringLiteral("state"), QStringLiteral("working") },
                                                       { QStringLiteral("facts"), QJsonObject {} }, { QStringLiteral("items"), QJsonArray { said } },
                                                       { QStringLiteral("queued"), QJsonArray { pictured, waiting(QStringLiteral("q2"), QStringLiteral("step")),
                                                                                       waiting(QStringLiteral("q3"), {}) } } } } });
        const auto queued = open.chat->queued();
        QCOMPARE(queued.size(), 3);
        QCOMPARE(queued.at(0).toMap().value(QStringLiteral("given")).toString(), QStringLiteral("turn"));
        QCOMPARE(queued.at(0).toMap().value(QStringLiteral("pictures")).toInt(), 2);
        QCOMPARE(queued.at(1).toMap().value(QStringLiteral("given")).toString(), QStringLiteral("step"));
        QCOMPARE(queued.at(2).toMap().value(QStringLiteral("given")).toString(), QString {});
        QCOMPARE(queued.at(2).toMap().value(QStringLiteral("pictures")).toInt(), 0);
        QCOMPARE(line(open.chat->lines(), QStringLiteral("person")).value(QStringLiteral("pictures")).toInt(), 1);
    }

    void aMessageTakesThePicturesOfTheDraftAndAnAnswerToTheAgentLeavesThemThere()
    {
        OpenChat open;
        auto* chat = open.chat;
        open.frame({ { QStringLiteral("snapshot"), QJsonObject { { QStringLiteral("revision"), 1 }, { QStringLiteral("state"), QStringLiteral("idle") },
                                                       { QStringLiteral("facts"), QJsonObject {} }, { QStringLiteral("items"), QJsonArray {} } } } });
        QTemporaryDir folder;
        const auto png = shotFile(folder, QStringLiteral("shot.png"), "PNG");
        const auto bmp = shotFile(folder, QStringLiteral("shot.bmp"), "BMP");
        QFile notes(folder.filePath(QStringLiteral("notes.txt")));
        QVERIFY(notes.open(QIODevice::WriteOnly));
        QVERIFY(notes.write("No picture here") > 0);
        notes.close();

        chat->addPictures({ png, bmp, QUrl::fromLocalFile(notes.fileName()) });
        QCOMPARE(chat->pictures().size(), 2);
        QCOMPARE(chat->pictureRefusal(), QStringLiteral("Kodosi cannot read this picture."));
        QVERIFY(chat->pictures().first().toMap().value(QStringLiteral("thumbnail")).toString().startsWith(QStringLiteral("data:image/png;base64,")));

        chat->setDraft(QStringLiteral("Look"));
        QVERIFY(chat->submit());
        const auto message = open.sent();
        QCOMPARE(message.value(QStringLiteral("do")).toString(), QStringLiteral("send"));
        QCOMPARE(message.value(QStringLiteral("text")).toString(), QStringLiteral("Look"));
        const auto pictures = message.value(QStringLiteral("pictures")).toArray();
        QCOMPARE(pictures.size(), 2);
        QFile kept(png.toLocalFile());
        QVERIFY(kept.open(QIODevice::ReadOnly));
        QCOMPARE(QByteArray::fromBase64(pictures.at(0).toString().toLatin1()), kept.readAll());
        QCOMPARE(QImage::fromData(QByteArray::fromBase64(pictures.at(1).toString().toLatin1()), "PNG").size(), shot().size());
        QVERIFY(chat->pictures().isEmpty());
        QVERIFY(chat->pictureRefusal().isEmpty());

        chat->addPictures(QList<QUrl>(9, png));
        QCOMPARE(chat->pictures().size(), 8);
        QCOMPARE(chat->pictureRefusal(), QStringLiteral("A message takes up to 8 pictures, 32 MB in all."));

        const QJsonObject ask { { QStringLiteral("id"), QStringLiteral("a1") },
            { QStringLiteral("body"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("question") }, { QStringLiteral("questions"), QJsonArray {} } } } };
        open.frame({ { QStringLiteral("update"), QJsonObject { { QStringLiteral("revision"), 2 },
                                                     { QStringLiteral("changes"), QJsonArray { QJsonObject { { QStringLiteral("change"), QStringLiteral("asks") },
                                                                                      { QStringLiteral("asks"), QJsonArray { ask } } } } } } } });
        QVERIFY(!chat->submit());
        chat->setDraft(QStringLiteral("Not this way"));
        QVERIFY(chat->submit());
        QCOMPARE(open.sent().value(QStringLiteral("do")).toString(), QStringLiteral("answer"));
        QCOMPARE(chat->pictures().size(), 8);
    }

    void aPasteTakesAPictureBeforeTheWordsThatCameWithIt()
    {
        OpenChat open;
        QByteArray png;
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        shot().save(&buffer, "PNG");
        auto* copied = new QMimeData;
        copied->setData(QStringLiteral("image/png"), png);
        copied->setText(QStringLiteral("https://kodosi.com/shot.png"));
        QGuiApplication::clipboard()->setMimeData(copied);
        QVERIFY(open.chat->pastePictures());
        QCOMPARE(open.chat->pictures().size(), 1);

        QGuiApplication::clipboard()->setText(QStringLiteral("Only words"));
        QVERIFY(!open.chat->pastePictures());
        QCOMPARE(open.chat->pictures().size(), 1);
    }
};

QTEST_MAIN(TerminalChatTest)
#include "tst_terminal_chat.moc"
