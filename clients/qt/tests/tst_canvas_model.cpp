#include "SessionFixture.hpp"
#include "canvas/ScreenVideo.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QBuffer>
#include <QClipboard>
#include <QGuiApplication>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QSignalSpy>
#include <QtTest/QTest>

#include <algorithm>
#include <memory>
#include <utility>

namespace {
class Commands final : public kodosi::CommandDispatcher, public kodosi::VideoWatcher {
public:
    QList<QJsonObject> values;
    QHash<QString, std::shared_ptr<kodosi::VideoSink>> watched;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    void watch(const QString& pipe, std::shared_ptr<kodosi::VideoSink> sink) override { watched.insert(pipe, std::move(sink)); }
    void unwatch(const QString& pipe) override { watched.remove(pipe); }
    QJsonObject request() const { return values.last().value(QStringLiteral("request")).toObject(); }
    // The newest command of the given type.
    QJsonObject last(const QString& type) const
    {
        for (auto value = values.crbegin(); value != values.crend(); ++value) {
            if (value->value(QStringLiteral("type")).toString() == type)
                return *value;
        }
        return {};
    }
    qsizetype count(const QString& type) const
    {
        return std::ranges::count_if(
            values, [&type](const QJsonObject& value) { return value.value(QStringLiteral("type")).toString() == type; });
    }
    // Answers the newest open, so that its video goes to the view, and opens the pipe.
    QString answerOpen(kodosi::CanvasDocuments& documents) const
    {
        const auto open = last(QStringLiteral("document.open"));
        const auto pipe = open.value(QStringLiteral("pipe")).toString();
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.result") },
            { QStringLiteral("requestId"), open.value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), QStringLiteral("document.open") } });
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } });
        return pipe;
    }
    // A message of the stream, as the runtime hands it to the view's decoder.
    void say(const QString& pipe, const QJsonObject& message) const
    {
        watched.value(pipe)->hand(QJsonDocument(message).toJson(QJsonDocument::Compact));
    }
    // The newest message of the given type that the app sent down a video pipe.
    QJsonObject told(const QString& type) const
    {
        QJsonObject newest;
        QString letters;
        for (const auto& value : values) {
            if (value.value(QStringLiteral("type")).toString() != QStringLiteral("document.send"))
                continue;
            letters += value.value(QStringLiteral("data")).toString();
            if (value.value(QStringLiteral("more")).toBool())
                continue;
            const auto message = QJsonDocument::fromJson(QByteArray::fromBase64(std::exchange(letters, {}).toLatin1()).mid(1)).object();
            if (message.value(QStringLiteral("type")).toString() == type)
                newest = message;
        }
        return newest;
    }
};

QJsonObject item(const QString& name, int changed)
{
    return { { QStringLiteral("key"), QStringLiteral("/work/") + name }, { QStringLiteral("name"), name },
        { QStringLiteral("kind"), QStringLiteral("page") }, { QStringLiteral("revision"), 1 }, { QStringLiteral("bytes"), 120 },
        { QStringLiteral("changed"), changed } };
}

QJsonObject terminal(QJsonObject canvas)
{
    auto entry = test::session(1);
    entry.insert(QStringLiteral("canvas"), canvas);
    return entry;
}

QJsonObject canvas(int sequence, QJsonArray items)
{
    return { { QStringLiteral("sequence"), sequence }, { QStringLiteral("items"), items } };
}

QJsonObject answer(const Commands& commands, QJsonObject answer)
{
    return { { QStringLiteral("type"), QStringLiteral("canvas.result") },
        { QStringLiteral("requestId"), commands.values.last().value(QStringLiteral("requestId")) },
        { QStringLiteral("operation"), QStringLiteral("canvas.request") }, { QStringLiteral("sessionId"), test::id(1) },
        { QStringLiteral("answer"), answer } };
}

// The screen's state.
QJsonObject screen(QJsonObject state)
{
    state.insert(QStringLiteral("source"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("display") }, { QStringLiteral("id"), 1 } });
    state.insert(QStringLiteral("width"), 1600);
    state.insert(QStringLiteral("height"), 1000);
    if (!state.contains(QStringLiteral("access")))
        state.insert(QStringLiteral("access"), QJsonObject { { QStringLiteral("screen"), true }, { QStringLiteral("input"), true } });
    return state;
}

// A decoder that the tests hand to a video pipe.
class Decoder final : public kodosi::VideoSink {
public:
    bool take(const Frame&) noexcept override { return true; }
    void hand(QByteArrayView) noexcept override { }
    void stale(bool) noexcept override { }
};

QJsonObject part(const QString& key, int revision, qsizetype offset, qsizetype total, const QByteArray& data)
{
    return { { QStringLiteral("answer"), QStringLiteral("part") }, { QStringLiteral("key"), key },
        { QStringLiteral("name"), key.section(QLatin1Char('/'), -1) }, { QStringLiteral("kind"), QStringLiteral("page") },
        { QStringLiteral("revision"), revision }, { QStringLiteral("offset"), offset }, { QStringLiteral("total"), total },
        { QStringLiteral("data"), QString::fromLatin1(data.toBase64()) } };
}
}

class CanvasModelTest final : public QObject {
    Q_OBJECT
private slots:
    void anItemIsReadInPartsAndAgainWhenItChanges()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        const auto key = QStringLiteral("/work/plan.html");
        sessions.apply(test::snapshot({ terminal(canvas(3, { item(QStringLiteral("plan.html"), 3) })) }));
        QSignalSpy loaded(&model, &kodosi::CanvasModel::loaded);
        QVERIFY(!model.load(id, key));
        QCOMPARE(commands.request().value(QStringLiteral("type")).toString(), QStringLiteral("read"));
        QCOMPARE(commands.request().value(QStringLiteral("offset")).toInt(), 0);
        model.apply(answer(commands, { { QStringLiteral("answer"), QStringLiteral("stale") },
                                         { QStringLiteral("key"), key }, { QStringLiteral("revision"), 2 } }));
        QCOMPARE(commands.request().value(QStringLiteral("revision")).toInt(), 2);
        model.apply(answer(commands, part(key, 2, 0, 9, QByteArrayLiteral("<p>Pl"))));
        QCOMPARE(commands.request().value(QStringLiteral("offset")).toInt(), 5);
        model.apply(answer(commands, part(key, 2, 5, 9, QByteArrayLiteral("an</"))));
        QCOMPARE(loaded.size(), 1);
        QCOMPARE(model.stored(id, key).value_or(QByteArray {}), QByteArrayLiteral("<p>Plan</"));
        QVERIFY(model.load(id, key));
        const auto address = QUrl(model.pageAddress(id, key));
        QCOMPARE(model.published(address).value_or(std::pair<QByteArray, QByteArray> {}).second, QByteArrayLiteral("<p>Plan</"));
        QCOMPARE(QUrl(model.pageAddress(id, key)), address);
        QVERIFY(!model.published(QUrl(QStringLiteral("kodosi-canvas://page/99"))));
        const auto before = commands.values.size();
        model.apply(answer(commands, part(key, 2, 0, 9, QByteArrayLiteral("x"))));
        QCOMPARE(commands.values.size(), before);
    }

    void askAboutThisTypesOneLineForThePersonToFinish()
    {
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), {}), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral(" round\neach\u001b[2J line\r")),
            QStringLiteral("About /work/plan.md: \"round each [2J line\" "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral("  ")), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("x"), QString(700, QLatin1Char('w'))).size(), 612);
    }

    void aLinkOnTheCanvasOfAnotherComputerOpensNoAddressOfThatComputerHere()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        auto remote = test::session(1, true);
        remote.insert(QStringLiteral("hostName"), QStringLiteral("studio"));
        sessions.apply(test::snapshot({ remote }));
        QSignalSpy refused(&model, &kodosi::CanvasModel::linkRefused);
        QSignalSpy asked(&model, &kodosi::CanvasModel::linkQuestionRequested);

        model.follow(test::id(1), QUrl(QStringLiteral("http://localhost:3000/")));
        QCOMPARE(refused.size(), 1);
        QCOMPARE(refused.first().at(0).toString(), test::id(1));
        QCOMPARE(refused.first().at(2).toString(), QStringLiteral("The address is on studio."));

        model.follow(test::id(1), QUrl(QStringLiteral("file:///home/ana/plan.md")));
        QCOMPARE(refused.last().at(2).toString(), QStringLiteral("The file is on studio."));

        model.follow(test::id(1), QUrl(QStringLiteral("vscode://file/home/ana/main.rs")));
        QCOMPARE(asked.size(), 1);
        QVERIFY(commands.values.isEmpty());
    }

    void aDocumentFrameComesBackWholeAndAGapOpensThePipeAgain()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        QSignalSpy opened(&documents, &kodosi::CanvasDocuments::opened);
        QSignalSpy received(&documents, &kodosi::CanvasDocuments::received);
        const auto pipe = documents.open(test::id(1), QStringLiteral("/work/plan.docx"));
        const auto part = [&pipe](int sequence, bool more, const QByteArray& data) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
                { QStringLiteral("sequence"), sequence }, { QStringLiteral("more"), more },
                { QStringLiteral("data"), QString::fromLatin1(data.toBase64()) } };
        };
        const QJsonObject open { { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } };
        documents.apply(open);
        QCOMPARE(opened.size(), 1);
        documents.apply(part(0, true, "Hel"));
        QCOMPARE(received.size(), 0);
        documents.apply(part(1, false, "lo"));
        QCOMPARE(received.size(), 1);
        QCOMPARE(received.first().at(1).toString(), QString::fromLatin1(QByteArray("Hello").toBase64()));
        documents.apply(part(3, false, "x"));
        const auto sent = commands.values.size();
        QCOMPARE(commands.values.at(sent - 2).value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
        QCOMPARE(commands.values.last().value(QStringLiteral("pipe")).toString(), pipe);
        documents.apply(part(4, false, "on its way"));
        documents.apply(open);
        documents.apply(part(0, false, "again"));
        QCOMPARE(received.size(), 2);
        QCOMPARE(received.last().at(1).toString(), QString::fromLatin1(QByteArray("again").toBase64()));
    }

    void aTerminalsBrowserHasOnePipeAndTheNewestViewTakesIt()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        QSignalSpy closed(&documents, &kodosi::CanvasDocuments::closed);
        const auto first = documents.openVideo(test::id(1), QStringLiteral("browser"), std::make_shared<Decoder>());
        QCOMPARE(commands.answerOpen(documents), first);
        const auto elsewhere = documents.openVideo(test::id(2), QStringLiteral("browser"), std::make_shared<Decoder>());
        QCOMPARE(commands.answerOpen(documents), elsewhere);
        const auto sent = commands.values.size();
        const auto second = documents.openVideo(test::id(1), QStringLiteral("browser"), std::make_shared<Decoder>());
        QCOMPARE(commands.values.size(), sent + 2);
        QCOMPARE(commands.values.at(sent).value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        QCOMPARE(commands.values.at(sent).value(QStringLiteral("pipe")).toString(), first);
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
        QCOMPARE(commands.values.last().value(QStringLiteral("pipe")).toString(), second);
        QCOMPARE(closed.size(), 1);
        QCOMPARE(closed.first().at(0).toString(), first);
        QCOMPARE(commands.watched.keys(), QStringList { elsewhere });
        commands.answerOpen(documents);
        QCOMPARE(QSet<QString>(commands.watched.keyBegin(), commands.watched.keyEnd()), (QSet<QString> { elsewhere, second }));
    }

    void aVideoPipeThatOpensAnewGoesToItsDecoderOnlyOnceTheNewOpenIsAnswered()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        const auto decoder = std::make_shared<Decoder>();
        const auto pipe = documents.openVideo(test::id(1), QStringLiteral("screen"), decoder);
        const auto earlier = commands.last(QStringLiteral("document.open"));
        QVERIFY(commands.watched.isEmpty());
        documents.renew(pipe);
        QCOMPARE(commands.values.at(commands.values.size() - 2).value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.result") },
            { QStringLiteral("requestId"), earlier.value(QStringLiteral("requestId")) },
            { QStringLiteral("operation"), QStringLiteral("document.open") } });
        QVERIFY(commands.watched.isEmpty());
        QCOMPARE(commands.answerOpen(documents), pipe);
        QVERIFY(commands.watched.value(pipe) == decoder);

        documents.renewPipes();
        QVERIFY(commands.watched.isEmpty());
        commands.answerOpen(documents);
        QVERIFY(commands.watched.value(pipe) == decoder);
        documents.close(pipe);
        QVERIFY(commands.watched.isEmpty());
    }

    void eachPipeOpensAgainAfterTheAppFellBehindAndAnEarlierAnswerClosesNothing()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        QSignalSpy received(&documents, &kodosi::CanvasDocuments::received);
        QSignalSpy closed(&documents, &kodosi::CanvasDocuments::closed);
        const auto pipe = documents.open(test::id(1), QStringLiteral("/work/plan.docx"));
        const auto registration = commands.values.last().value(QStringLiteral("requestId"));
        const QJsonObject open { { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } };
        const auto frame = [&pipe](const QByteArray& data) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
                { QStringLiteral("sequence"), 0 }, { QStringLiteral("more"), false },
                { QStringLiteral("data"), QString::fromLatin1(data.toBase64()) } };
        };
        documents.apply(open);
        documents.renewPipes();
        const auto sent = commands.values.size();
        QCOMPARE(commands.values.at(sent - 2).value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
        QCOMPARE(commands.values.last().value(QStringLiteral("pipe")).toString(), pipe);
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.error") }, { QStringLiteral("requestId"), registration },
            { QStringLiteral("operation"), QStringLiteral("document.open") } });
        QVERIFY(closed.isEmpty());
        documents.apply(frame("before"));
        QVERIFY(received.isEmpty());
        documents.apply(open);
        documents.apply(frame("after"));
        QCOMPARE(received.size(), 1);
        QCOMPARE(received.first().at(1).toString(), QString::fromLatin1(QByteArray("after").toBase64()));
    }

    void aZoomKeepsThePointUnderThePointerAndAsksTheScreenForThatPartOfItsSource()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::ScreenVideo video;
        video.setSize(QSizeF(800, 500));
        video.setDocuments(&documents);
        video.setSessionId(test::id(1));
        video.setActive(true);
        const auto pipe = commands.answerOpen(documents);
        const auto state = screen({});
        commands.say(pipe, state);
        QTRY_VERIFY(!video.source().isEmpty());
        const auto pointed = [&] {
            video.pointer(QStringLiteral("move"), 600, 120, Qt::NoButton, 0, 0);
            const auto input = commands.told(QStringLiteral("input")).value(QStringLiteral("input")).toObject();
            return QPointF(input.value(QStringLiteral("x")).toDouble(), input.value(QStringLiteral("y")).toDouble());
        };

        const auto before = pointed();
        video.zoom(2, 600, 120);

        QCOMPARE(pointed(), before);
        const auto look = commands.told(QStringLiteral("look"));
        const auto region = look.value(QStringLiteral("region")).toObject();
        QCOMPARE(look.value(QStringLiteral("source")).toObject(), state.value(QStringLiteral("source")).toObject());
        QCOMPARE(QRectF(region.value(QStringLiteral("x")).toDouble(), region.value(QStringLiteral("y")).toDouble(),
                     region.value(QStringLiteral("width")).toDouble(), region.value(QStringLiteral("height")).toDouble()),
            QRectF(600, 120, 800, 500));
    }

    void aScreenSaysTheReasonThatMattersMostWhenItShowsNothing()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::ScreenVideo video;
        video.setSize(QSizeF(800, 500));
        video.setDocuments(&documents);
        video.setSessionId(test::id(1));
        video.setActive(true);
        const auto pipe = commands.answerOpen(documents);
        const QJsonObject unrecorded { { QStringLiteral("screen"), false }, { QStringLiteral("input"), true } };
        commands.say(pipe, screen({ { QStringLiteral("away"), QStringLiteral("locked") }, { QStringLiteral("access"), unrecorded } }));
        QTRY_COMPARE(video.showing(), QStringLiteral("locked"));
        video.setOffline(true);
        QCOMPARE(video.showing(), QStringLiteral("offline"));
        video.setShared(false);
        QCOMPARE(video.showing(), QStringLiteral("offline"));
        video.setOffline(false);
        QCOMPARE(video.showing(), QStringLiteral("sharingOff"));

        video.setShared(true);
        const auto again = commands.answerOpen(documents);
        commands.say(again, screen({ { QStringLiteral("access"), unrecorded } }));
        QTRY_COMPARE(video.showing(), QStringLiteral("recording"));
        commands.say(again, screen({}));
        QTRY_COMPARE(video.showing(), QStringLiteral("waiting"));
        QTest::ignoreMessage(QtWarningMsg, "A canvas video stopped: Kodosi Screen stopped.");
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.lost") }, { QStringLiteral("pipe"), again },
            { QStringLiteral("message"), QStringLiteral("Kodosi Screen stopped.") } });
        QCOMPARE(video.showing(), QStringLiteral("failed"));
    }

    void aScreenThatIsNotSharedHasNoPipeUntilItIs()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::ScreenVideo video;
        video.setDocuments(&documents);
        video.setSessionId(test::id(1));
        video.setActive(true);
        const auto pipe = commands.answerOpen(documents);
        QCOMPARE(commands.watched.keys(), QStringList { pipe });

        video.setShared(false);
        QVERIFY(commands.watched.isEmpty());
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        QCOMPARE(commands.values.last().value(QStringLiteral("pipe")).toString(), pipe);
        const auto opens = commands.count(QStringLiteral("document.open"));
        video.setActive(false);
        video.setActive(true);
        video.retry();
        QCOMPARE(commands.count(QStringLiteral("document.open")), opens);

        video.setShared(true);
        QCOMPARE(commands.count(QStringLiteral("document.open")), opens + 1);
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
    }

    void aTerminalsScreenComesBackAsItsViewLeftIt()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel canvas(commands, sessions);
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        sessions.apply(test::snapshot({ terminal({ { QStringLiteral("sequence"), 1 },
            { QStringLiteral("screen"), QJsonObject { { QStringLiteral("shared"), true } } } }) }));
        const auto open = [&](kodosi::ScreenVideo& video) {
            video.setSize(QSizeF(800, 500));
            video.setDocuments(&documents);
            video.setCanvas(&canvas);
            video.setSessionId(test::id(1));
        };
        {
            kodosi::ScreenVideo video;
            open(video);
            video.setActive(true);
            commands.say(commands.answerOpen(documents), screen({}));
            QTRY_VERIFY(!video.source().isEmpty());
            video.zoom(2, 600, 120);
        }
        commands.values.clear();

        kodosi::ScreenVideo video;
        open(video);
        QVERIFY(video.zoomed());
        video.setActive(true);
        commands.say(commands.answerOpen(documents), screen({}));
        QTRY_VERIFY(!commands.told(QStringLiteral("look")).isEmpty());
        const auto region = commands.told(QStringLiteral("look")).value(QStringLiteral("region")).toObject();
        QCOMPARE(QRectF(region.value(QStringLiteral("x")).toDouble(), region.value(QStringLiteral("y")).toDouble(),
                     region.value(QStringLiteral("width")).toDouble(), region.value(QStringLiteral("height")).toDouble()),
            QRectF(600, 120, 800, 500));
    }

    void textCopiedOnTheComputerComesHereAndAPasteThereTakesTextFromHere()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::ScreenVideo video;
        video.setDocuments(&documents);
        video.setSessionId(test::id(1));
        video.setActive(true);
        QSignalSpy copied(&video, &kodosi::ScreenVideo::copied);
        commands.say(commands.answerOpen(documents), screen({ { QStringLiteral("copied"), QStringLiteral("from the computer") } }));
        QTRY_COMPARE(copied.size(), 1);
        QCOMPARE(QGuiApplication::clipboard()->text(), QStringLiteral("from the computer"));

        QGuiApplication::clipboard()->setText(QStringLiteral("from here"));
        video.giveClipboard();
        QCOMPARE(commands.told(QStringLiteral("clipboard")).value(QStringLiteral("text")).toString(), QStringLiteral("from here"));
    }

    void aBrowserThatFellAsleepWakesOnceUntilItFallsAsleepAgain()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        const auto browser = [](int sequence, const QString& state, const QString& problem = {}) {
            QJsonObject shown { { QStringLiteral("state"), state }, { QStringLiteral("changed"), 1 }, { QStringLiteral("width"), 800 },
                { QStringLiteral("height"), 600 } };
            if (!problem.isEmpty())
                shown.insert(QStringLiteral("problem"), problem);
            return terminal({ { QStringLiteral("sequence"), sequence }, { QStringLiteral("browser"), shown } });
        };
        const auto wakes = [&commands] {
            return static_cast<int>(std::ranges::count_if(commands.values, [](const QJsonObject& value) {
                const auto action = value.value(QStringLiteral("request")).toObject().value(QStringLiteral("action")).toObject();
                return action.value(QStringLiteral("do")).toString() == QStringLiteral("wake");
            }));
        };

        sessions.apply(test::snapshot({ browser(1, QStringLiteral("asleep")) }));
        model.wakeAsleep(id);
        model.wakeAsleep(id);
        QCOMPARE(wakes(), 1);
        sessions.apply(test::snapshot({ browser(2, QStringLiteral("starting")) }));
        sessions.apply(test::snapshot({ browser(3, QStringLiteral("asleep")) }));
        model.wakeAsleep(id);
        QCOMPARE(wakes(), 2);
        sessions.apply(test::snapshot({ browser(4, QStringLiteral("awake")) }));
        sessions.apply(test::snapshot({ browser(5, QStringLiteral("asleep"), QStringLiteral("Kodosi Browser did not start.")) }));
        model.wakeAsleep(id);
        QCOMPARE(wakes(), 2);
    }

    void aLargeFrameGoesInPartsOfWholeLetterGroups()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        const auto pipe = documents.open(test::id(1), QStringLiteral("/work/plan.docx"));
        documents.send(pipe, QString(600 * 1024, u'A'));
        QCOMPARE(commands.values.size(), 3);
        const auto first = commands.values.at(1);
        QVERIFY(first.value(QStringLiteral("more")).toBool());
        QCOMPARE(first.value(QStringLiteral("data")).toString().size() % 4, 0);
        QVERIFY(!commands.values.at(2).value(QStringLiteral("more")).toBool());
    }

    void aComputersPointerBecomesACursorAtAWholeScaleWhateverTheComputerSays()
    {
        // A display at half a pixel per point sends a 32-pixel picture that is 64 points wide.
        const auto half = kodosi::cursorPicture(QSizeF(64, 64), QPointF(10.4, 20.6), 1);
        QCOMPARE(half.pixels, QSize(64, 64));
        QCOMPARE(half.scale, 1);
        QCOMPARE(half.hotspot, QPoint(10, 21));
        const auto fractional = kodosi::cursorPicture(QSizeF(24.5, 24.5), QPointF(3, 3), 1.5);
        QCOMPARE(fractional.pixels, QSize(50, 50));
        QCOMPARE(fractional.scale, 2);

        for (const qreal said : { qQNaN(), qInf(), -qInf(), -1.0, 0.0, 0.25, 1e300 }) {
            const auto cursor = kodosi::cursorPicture(QSizeF(said, said), QPointF(said, said), said);
            QVERIFY2(cursor.scale >= 1 && cursor.scale <= 4, qPrintable(QString::number(said)));
            QVERIFY(cursor.pixels.width() >= cursor.scale && cursor.pixels.width() % cursor.scale == 0);
            QVERIFY(cursor.pixels.height() >= cursor.scale && cursor.pixels.height() % cursor.scale == 0);
            QVERIFY(cursor.pixels.width() <= 256 * cursor.scale && cursor.pixels.height() <= 256 * cursor.scale);
            QVERIFY(cursor.hotspot.x() >= 0 && cursor.hotspot.x() < cursor.pixels.width() / cursor.scale);
            QVERIFY(cursor.hotspot.y() >= 0 && cursor.hotspot.y() < cursor.pixels.height() / cursor.scale);
        }
    }

    void aWheelsNotchScrollsByItsAngleAndATouchpadByItsPixels()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, commands, sessions);
        kodosi::ScreenVideo video;
        video.setSize(QSizeF(800, 500));
        video.setDocuments(&documents);
        video.setSessionId(test::id(1));
        video.setActive(true);
        commands.say(commands.answerOpen(documents), screen({}));
        QTRY_VERIFY(!video.source().isEmpty());
        const auto scrolled = [&](QPoint pixels, QPoint angle, Qt::ScrollPhase phase) {
            video.wheel(600, 120, pixels, angle, phase, 0);
            return commands.told(QStringLiteral("input")).value(QStringLiteral("input")).toObject().value(QStringLiteral("deltaY")).toDouble();
        };

        // A notch down on Wayland: an angle of 120 and 15 pixels.
        QCOMPARE(scrolled(QPoint(0, -15), QPoint(0, -120), Qt::NoScrollPhase), 100.0);
        QCOMPARE(scrolled(QPoint(), QPoint(0, 120), Qt::NoScrollPhase), -100.0);
        QCOMPARE(scrolled(QPoint(0, -7), QPoint(0, -84), Qt::ScrollUpdate), 7.0);
    }
};

QTEST_MAIN(CanvasModelTest)
#include "tst_canvas_model.moc"
