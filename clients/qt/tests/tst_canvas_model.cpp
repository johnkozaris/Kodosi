#include "SessionFixture.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QBuffer>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest/QTest>

namespace {
class Commands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    QJsonObject request() const { return values.last().value(QStringLiteral("request")).toObject(); }
};

QJsonObject item(const QString& name, int changed, int revision = 1)
{
    return { { QStringLiteral("key"), QStringLiteral("/work/") + name }, { QStringLiteral("name"), name },
        { QStringLiteral("kind"), name.endsWith(QStringLiteral(".png")) ? QStringLiteral("picture") : QStringLiteral("page") },
        { QStringLiteral("revision"), revision }, { QStringLiteral("bytes"), 120 }, { QStringLiteral("changed"), changed } };
}

QJsonObject terminal(QJsonObject canvas)
{
    auto entry = test::session(1);
    if (!canvas.isEmpty())
        entry.insert(QStringLiteral("canvas"), canvas);
    return entry;
}

QJsonObject canvas(int sequence, QJsonArray items, QJsonObject browser = {})
{
    QJsonObject value { { QStringLiteral("sequence"), sequence }, { QStringLiteral("items"), items } };
    if (!browser.isEmpty())
        value.insert(QStringLiteral("browser"), browser);
    return value;
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
    void theTerminalListKeepsAValidCanvasAndRefusesABrokenOne()
    {
        kodosi::SessionCatalogModel sessions;
        const QJsonObject browser { { QStringLiteral("state"), QStringLiteral("asleep") }, { QStringLiteral("changed"), 2 },
            { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } };
        sessions.apply(test::snapshot({ terminal(canvas(9, { item(QStringLiteral("compare.html"), 4) }, browser)) }));
        const auto shown = sessions.presentationForSession(test::id(1)).value(QStringLiteral("canvas")).toMap();
        QCOMPARE(shown.value(QStringLiteral("items")).toList().size(), 1);
        QCOMPARE(shown.value(QStringLiteral("browser")).toMap().value(QStringLiteral("state")).toString(), QStringLiteral("asleep"));
        auto broken = item(QStringLiteral("plan.html"), 5);
        broken.insert(QStringLiteral("kind"), QStringLiteral("program"));
        sessions.apply(test::snapshot({ terminal(canvas(10, { broken })) }));
        QCOMPARE(sessions.authorityState(), kodosi::SessionCatalogModel::Failed);
        sessions.apply(test::snapshot({ terminal({}) }));
        QVERIFY(!sessions.presentationForSession(test::id(1)).value(QStringLiteral("canvas")).isValid());
    }

    void aCanvasShowsNewsOnlyForWhatChangedAfterThisDeviceSawIt()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        sessions.apply(test::snapshot({ terminal(canvas(4, { item(QStringLiteral("compare.html"), 4) })) }));
        QVERIFY(!model.hasNews(id));
        sessions.apply(test::snapshot({ terminal(
            canvas(6, { item(QStringLiteral("plan.md"), 6), item(QStringLiteral("compare.html"), 4) })) }));
        QVERIFY(model.hasNews(id));
        QCOMPARE(model.current(id), QStringLiteral("/work/plan.md"));
        model.saw(id, QStringLiteral("/work/plan.md"));
        QVERIFY(!model.hasNews(id));
        model.pick(id, QStringLiteral("/work/compare.html"));
        sessions.apply(test::snapshot({ terminal(
            canvas(8, { item(QStringLiteral("plan.md"), 8, 2), item(QStringLiteral("compare.html"), 4) })) }));
        QCOMPARE(model.current(id), QStringLiteral("/work/compare.html"));
        QVERIFY(model.isNew(id, QStringLiteral("/work/plan.md")));
        sessions.apply(test::snapshot({ terminal(canvas(9, { item(QStringLiteral("plan.md"), 8, 2) })) }));
        QCOMPARE(model.current(id), QStringLiteral("/work/plan.md"));
    }

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
        QCOMPARE(address.scheme(), QStringLiteral("kodosi-canvas"));
        QCOMPARE(model.published(address).value_or(std::pair<QByteArray, QByteArray> {}).second, QByteArrayLiteral("<p>Plan</"));
        QCOMPARE(QUrl(model.pageAddress(id, key)), address);
        QVERIFY(!model.published(QUrl(QStringLiteral("kodosi-canvas://page/99"))));
        const auto before = commands.values.size();
        model.apply(answer(commands, part(key, 2, 0, 9, QByteArrayLiteral("x"))));
        QCOMPARE(commands.values.size(), before);
    }

    void aPointOfTheViewBecomesAPointOfTheBrowser()
    {
        const QSizeF wide(1000, 500);
        const QSizeF browser(1280, 720);
        QCOMPARE(kodosi::CanvasModel::browserPoint(QPointF(500, 250), wide, browser), QPointF(640, 360));
        QVERIFY(!kodosi::CanvasModel::browserPoint(QPointF(10, 250), wide, browser));
        QCOMPARE(kodosi::CanvasModel::pointInView(QPointF(0, 0), wide, browser), QPointF(55.55555555555554, 0));
    }

    void theBrowserKnowsTheKeysOfAUsKeyboard()
    {
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_A)->code, QStringLiteral("KeyA"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Exclam)->code, QStringLiteral("Digit1"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Return)->named, QStringLiteral("Enter"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Left)->windows, 37);
        QVERIFY(!kodosi::CanvasModel::browserKey(Qt::Key_Shift));
    }

    void askAboutThisTypesOneLineForThePersonToFinish()
    {
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), {}), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral(" round\neach\u001b[2J line\r")),
            QStringLiteral("About /work/plan.md: \"round each [2J line\" "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral("  ")), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("x"), QString(700, QLatin1Char('w'))).size(), 612);
    }

    void aPageFollowsOnlyWebAndMailLinks()
    {
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("https://kodosi.com/docs"))));
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("mailto:ana@example.com"))));
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("http://localhost:3000"))));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("file:///etc/passwd"))));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("javascript:alert(1)"))));
    }

    void aPartClosesOnThisDeviceUntilItChanges()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        const auto files = QStringLiteral("files");
        const auto web = QStringLiteral("browser");
        QJsonObject browser { { QStringLiteral("state"), QStringLiteral("asleep") }, { QStringLiteral("changed"), 3 },
            { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } };
        sessions.apply(test::snapshot({ terminal(canvas(1, { item(QStringLiteral("plan.md"), 2) }, browser)) }));
        QCOMPARE(model.sections(id), (QStringList { files, web }));
        QCOMPARE(model.current(id), web);
        model.close(id, web);
        QCOMPARE(model.sections(id), QStringList { files });
        QCOMPARE(model.closedSections(id), QStringList { web });
        QCOMPARE(model.current(id), QStringLiteral("/work/plan.md"));
        browser.insert(QStringLiteral("changed"), 4);
        sessions.apply(test::snapshot({ terminal(canvas(2, { item(QStringLiteral("plan.md"), 2) }, browser)) }));
        QCOMPARE(model.sections(id), (QStringList { files, web }));
        model.showShelf(id, true);
        model.close(id, files);
        QVERIFY(!model.showsShelf(id));
        QCOMPARE(model.closedSections(id), QStringList { files });
        model.show(id, files);
        QCOMPARE(model.current(id), QStringLiteral("/work/plan.md"));
        model.showShelf(id, true);
        model.pick(id, QStringLiteral("/work/plan.md"));
        QVERIFY(!model.showsShelf(id));
    }

    void aPersonOpensTheBrowserAndSeesItWhenItComes()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        sessions.apply(test::snapshot({ terminal(canvas(1, { item(QStringLiteral("plan.md"), 2) })) }));
        QCOMPARE(model.closedSections(id), QStringList { QStringLiteral("browser") });
        model.openBrowser(id);
        QVERIFY(model.opensBrowser(id));
        QCOMPARE(commands.request().value(QStringLiteral("action")).toObject().value(QStringLiteral("do")).toString(),
            QStringLiteral("wake"));
        const QJsonObject browser { { QStringLiteral("state"), QStringLiteral("starting") }, { QStringLiteral("changed"), 1 },
            { QStringLiteral("width"), 1280 }, { QStringLiteral("height"), 720 } };
        sessions.apply(test::snapshot({ terminal(canvas(2, { item(QStringLiteral("plan.md"), 2) }, browser)) }));
        QVERIFY(!model.opensBrowser(id));
        QCOMPARE(model.current(id), QStringLiteral("browser"));
    }

    void oneCanvasOpensWithItsTerminalAloneAndHidesWithoutLeavingIt()
    {
        QTemporaryDir directory;
        kodosi::SessionCatalogModel sessions;
        kodosi::DesktopStateModel desktop(
            std::make_unique<QSettings>(directory.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat));
        desktop.attachSessionCatalog(&sessions);
        const auto id = test::id(1);
        sessions.apply(test::snapshot({ terminal(canvas(1, { item(QStringLiteral("plan.md"), 1) })) }));
        desktop.openCanvas(id);
        QCOMPARE(desktop.canvasSessionId(), id);
        QCOMPARE(desktop.selectedSessionId(), id);
        QCOMPARE(desktop.stageLayoutMode(), kodosi::DesktopStateModel::Focus);
        desktop.hideCanvas();
        QVERIFY(desktop.canvasSessionId().isEmpty());
        QCOMPARE(desktop.stageLayoutMode(), kodosi::DesktopStateModel::Focus);
        desktop.openCanvas(id);
        sessions.apply(test::snapshot({ terminal({}) }));
        QVERIFY(desktop.canvasSessionId().isEmpty());
        sessions.apply(test::snapshot({ terminal(canvas(2, { item(QStringLiteral("plan.md"), 2) })) }));
        QCOMPARE(desktop.canvasSessionId(), id);
        desktop.toggleFocusForSelectedSession();
        QVERIFY(desktop.canvasSessionId().isEmpty());
        QCOMPARE(desktop.stageLayoutMode(), kodosi::DesktopStateModel::Grid);
        sessions.apply(test::snapshot({ terminal({}) }));
        sessions.apply(test::snapshot({ terminal(canvas(3, { item(QStringLiteral("plan.md"), 3) })) }));
        QVERIFY(desktop.canvasSessionId().isEmpty());
        desktop.openCanvas(id);
        QVERIFY(desktop.unstageSession(id));
        QVERIFY(desktop.canvasSessionId().isEmpty());
    }

    void aCanvasLeftOpenIsOpenAgainWhenThePersonComesBack()
    {
        QTemporaryDir directory;
        kodosi::SessionCatalogModel sessions;
        kodosi::DesktopStateModel desktop(
            std::make_unique<QSettings>(directory.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat));
        desktop.attachSessionCatalog(&sessions);
        const auto shown = test::id(1);
        const auto plain = test::id(2);
        sessions.apply(test::snapshot({ terminal(canvas(1, { item(QStringLiteral("plan.md"), 1) })), test::session(2) }));
        QVERIFY(desktop.selectSession(plain));
        desktop.openCanvas(shown);
        QVERIFY(desktop.selectSession(plain));
        QVERIFY(desktop.canvasSessionId().isEmpty());
        QVERIFY(desktop.selectSession(shown));
        QCOMPARE(desktop.canvasSessionId(), shown);
        QVERIFY(desktop.selectSession(plain));
        desktop.toggleFocusForSelectedSession();
        QVERIFY(desktop.selectSession(shown));
        desktop.toggleFocusForSelectedSession();
        QCOMPARE(desktop.canvasSessionId(), shown);
        desktop.hideCanvas();
        QVERIFY(desktop.selectSession(plain));
        QVERIFY(desktop.selectSession(shown));
        QVERIFY(desktop.canvasSessionId().isEmpty());
    }

    void aPersonAddsAFileInPartsAndTheCanvasShowsIt()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        QTemporaryDir directory;
        QFile file(directory.filePath(QStringLiteral("plan.md")));
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write(QByteArray(300 * 1024, 'a'));
        file.close();
        model.attach(id, QUrl::fromLocalFile(file.fileName()));
        QCOMPARE(commands.request().value(QStringLiteral("type")).toString(), QStringLiteral("attach"));
        QCOMPARE(commands.request().value(QStringLiteral("name")).toString(), QStringLiteral("plan.md"));
        QVERIFY(!commands.request().value(QStringLiteral("last")).toBool());
        model.apply(answer(commands, { { QStringLiteral("answer"), QStringLiteral("done") } }));
        QCOMPARE(commands.request().value(QStringLiteral("offset")).toInt(), 256 * 1024);
        QVERIFY(commands.request().value(QStringLiteral("last")).toBool());
        sessions.apply(test::snapshot({ terminal(canvas(2, { item(QStringLiteral("plan.md"), 2) })) }));
        model.apply(answer(commands,
            { { QStringLiteral("answer"), QStringLiteral("attached") }, { QStringLiteral("key"), QStringLiteral("/work/plan.md") } }));
        QCOMPARE(model.current(id), QStringLiteral("/work/plan.md"));
    }

    void aDocumentFrameComesBackWholeAndAGapBreaksThePipe()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, sessions);
        QSignalSpy opened(&documents, &kodosi::CanvasDocuments::opened);
        QSignalSpy received(&documents, &kodosi::CanvasDocuments::received);
        QSignalSpy lost(&documents, &kodosi::CanvasDocuments::lost);
        const auto pipe = documents.open(test::id(1), QStringLiteral("/work/plan.docx"));
        const auto request = commands.values.last().value(QStringLiteral("requestId"));
        documents.apply({ { QStringLiteral("type"), QStringLiteral("document.result") }, { QStringLiteral("requestId"), request } });
        QCOMPARE(opened.size(), 1);
        const auto part = [&pipe](int sequence, bool more, const QByteArray& data) {
            return QJsonObject { { QStringLiteral("type"), QStringLiteral("document.frame") }, { QStringLiteral("pipe"), pipe },
                { QStringLiteral("sequence"), sequence }, { QStringLiteral("more"), more },
                { QStringLiteral("data"), QString::fromLatin1(data.toBase64()) } };
        };
        documents.apply(part(0, true, "Hel"));
        QCOMPARE(received.size(), 0);
        documents.apply(part(1, false, "lo"));
        QCOMPARE(received.size(), 1);
        QCOMPARE(received.first().at(1).toString(), QString::fromLatin1(QByteArray("Hello").toBase64()));
        documents.apply(part(3, false, "x"));
        QCOMPARE(lost.size(), 1);
        documents.send(pipe, QStringLiteral("AAAA"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("document.open"));
    }

    void aDocumentHearsWhenItsTerminalRunsAgain()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, sessions);
        QSignalSpy reachable(&documents, &kodosi::CanvasDocuments::reachable);
        auto remote = test::session(1, true);
        remote.insert(QStringLiteral("status"), QStringLiteral("reconnecting"));
        sessions.apply(test::snapshot({ remote }));
        QCOMPARE(reachable.size(), 0);
        remote.insert(QStringLiteral("status"), QStringLiteral("running"));
        sessions.apply(test::snapshot({ remote }));
        QCOMPARE(reachable.size(), 1);
        QCOMPARE(reachable.first().first().toString(), test::id(1));
        sessions.apply(test::snapshot({ remote }));
        QCOMPARE(reachable.size(), 1);
    }

    void aLargeFrameGoesInPartsOfWholeLetterGroups()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasDocuments documents(commands, sessions);
        const auto pipe = documents.open(test::id(1), QStringLiteral("/work/plan.docx"));
        documents.send(pipe, QString(600 * 1024, u'A'));
        QCOMPARE(commands.values.size(), 3);
        const auto first = commands.values.at(1);
        QVERIFY(first.value(QStringLiteral("more")).toBool());
        QCOMPARE(first.value(QStringLiteral("data")).toString().size() % 4, 0);
        QVERIFY(!commands.values.at(2).value(QStringLiteral("more")).toBool());
    }

    void aBrowserStepShowsWhatItDidAndAsksForTheNewStatus()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto answer = [&](const QString& type, const QString& operation, QJsonObject fields) {
            fields.insert(QStringLiteral("type"), type);
            fields.insert(QStringLiteral("operation"), operation);
            fields.insert(QStringLiteral("requestId"), commands.values.last().value(QStringLiteral("requestId")));
            model.applyBrowser(fields);
        };
        model.openBrowserSettings();
        answer(QStringLiteral("browser.result"), QStringLiteral("browser.status"),
            { { QStringLiteral("sources"), QJsonArray { QStringLiteral("Chrome"), QStringLiteral("Firefox") } },
                { QStringLiteral("sites"), 0 } });
        QVERIFY(model.browserKnown());
        QCOMPARE(model.browserSources(), QStringList({ QStringLiteral("Chrome"), QStringLiteral("Firefox") }));
        QVERIFY(model.browserEngine().isEmpty());

        model.copySignIns(QStringLiteral("Chrome"));
        QCOMPARE(model.browserWork(), QStringLiteral("Chrome"));
        model.forgetSignIns();
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("browser.copy"));
        answer(QStringLiteral("browser.result"), QStringLiteral("browser.copy"), { { QStringLiteral("sites"), 12 } });
        QVERIFY(model.browserWork().isEmpty());
        QCOMPARE(model.browserDone().value(QStringLiteral("Chrome")).toString(), QStringLiteral("Copied for 12 sites"));
        QCOMPARE(commands.values.last().value(QStringLiteral("type")).toString(), QStringLiteral("browser.status"));
        answer(QStringLiteral("browser.result"), QStringLiteral("browser.status"),
            { { QStringLiteral("sources"), QJsonArray { QStringLiteral("Chrome") } }, { QStringLiteral("sites"), 12 },
                { QStringLiteral("engine"), QStringLiteral("Chromium 156.0.8078.47") } });
        QCOMPARE(model.browserSites(), 12);
        QCOMPARE(model.browserEngine(), QStringLiteral("Chromium 156.0.8078.47"));

        model.forgetSignIns();
        answer(QStringLiteral("browser.result"), QStringLiteral("browser.forget"), { { QStringLiteral("sites"), 12 } });
        QVERIFY(model.browserDone().isEmpty());

        model.removeBrowsingData();
        answer(QStringLiteral("browser.error"), QStringLiteral("browser.clear"),
            { { QStringLiteral("message"), QStringLiteral("The profile could not go.") } });
        QCOMPARE(model.browserProblem(), QStringLiteral("The profile could not go."));
        QVERIFY(!model.browserDone().contains(QStringLiteral("clear")));
    }
};

QTEST_MAIN(CanvasModelTest)
#include "tst_canvas_model.moc"
