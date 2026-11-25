#include "SessionFixture.hpp"
#include "canvas/ScreenVideo.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QBuffer>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QtTest/QTest>

#include <utility>

namespace {
class Commands final : public kodosi::CommandDispatcher, public kodosi::PipeRouter {
public:
    QList<QJsonObject> values;
    QStringList routes;
    QHash<QString, std::pair<QObject*, Read>> readers;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    void route(const QString& pipe, QObject& reader, Read read) override
    {
        routes.append(pipe);
        readers.insert(pipe, { &reader, std::move(read) });
    }
    void unroute(const QString& pipe) override { routes.removeAll(pipe); }
    QJsonObject request() const { return values.last().value(QStringLiteral("request")).toObject(); }
    // Hands a pipe's event to its reader on the reader's thread, as the runtime bridge does.
    void deliver(const QString& pipe, const QJsonObject& event) const
    {
        const auto [reader, read] = readers.value(pipe);
        QMetaObject::invokeMethod(reader, [read, bytes = QJsonDocument(event).toJson(QJsonDocument::Compact)] { read(bytes, false); });
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
        QObject earlier;
        QObject later;
        QObject other;
        const auto first = documents.openReader(test::id(1), QStringLiteral("browser"), earlier, {});
        const auto elsewhere = documents.openReader(test::id(2), QStringLiteral("browser"), other, {});
        const auto sent = commands.values.size();
        const auto second = documents.openReader(test::id(1), QStringLiteral("browser"), later, {});
        QCOMPARE(commands.values.size(), sent + 2);
        QCOMPARE(commands.values.at(sent).value(QStringLiteral("type")).toString(), QStringLiteral("document.close"));
        QCOMPARE(commands.values.at(sent).value(QStringLiteral("pipe")).toString(), first);
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
        QCOMPARE(commands.values.last().value(QStringLiteral("pipe")).toString(), second);
        QCOMPARE(closed.size(), 1);
        QCOMPARE(closed.first().at(0).toString(), first);
        QCOMPARE(commands.routes, (QStringList { elsewhere, second }));
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
        const auto pipe = commands.routes.last();
        const QJsonObject state { { QStringLiteral("source"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("display") }, { QStringLiteral("id"), 1 } } },
            { QStringLiteral("width"), 1600 }, { QStringLiteral("height"), 1000 },
            { QStringLiteral("access"), QJsonObject { { QStringLiteral("screen"), true }, { QStringLiteral("input"), true } } } };
        commands.deliver(pipe, { { QStringLiteral("type"), QStringLiteral("document.opened") }, { QStringLiteral("pipe"), pipe } });
        commands.deliver(pipe, { { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
            { QStringLiteral("sequence"), 0 }, { QStringLiteral("more"), false },
            { QStringLiteral("data"), QString::fromLatin1((QByteArray(1, 0) + QJsonDocument(state).toJson()).toBase64()) } });
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
};

QTEST_MAIN(CanvasModelTest)
#include "tst_canvas_model.moc"
