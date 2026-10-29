#include "SessionFixture.hpp"
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
        const QSizeF picture(1280, 720);
        QCOMPARE(kodosi::CanvasModel::browserPoint(QPointF(500, 250), wide, picture, picture), QPoint(640, 360));
        QVERIFY(!kodosi::CanvasModel::browserPoint(QPointF(10, 250), wide, picture, picture));
        QCOMPARE(kodosi::CanvasModel::browserPoint(QPointF(500, 250), wide, QSizeF(2560, 1440), picture), QPoint(640, 360));
        QCOMPARE(kodosi::CanvasModel::browserPoint(QPointF(75.8, 103.3), QSizeF(451, 690), QSizeF(500, 690), QSizeF(451, 690)),
            QPoint(84, 77));
    }

    void theBrowserGetsTheKeysOfAUsKeyboardAndOneBatchOfInputAtATime()
    {
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_A)->code, QStringLiteral("KeyA"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Exclam)->code, QStringLiteral("Digit1"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Return)->named, QStringLiteral("Enter"));
        QCOMPARE(kodosi::CanvasModel::browserKey(Qt::Key_Left)->windows, 37);
        QVERIFY(!kodosi::CanvasModel::browserKey(Qt::Key_Shift));

        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        model.setBrowserView(id, 800, 600);
        QVERIFY(model.key(id, Qt::Key_A, QStringLiteral("a"), Qt::NoModifier, true));
        QCOMPARE(commands.values.size(), 1);
        const auto events = commands.request().value(QStringLiteral("events")).toArray();
        QCOMPARE(events.first().toObject().value(QStringLiteral("text")).toString(), QStringLiteral("a"));
        QCOMPARE(commands.request().value(QStringLiteral("width")).toInt(), 800);
        QVERIFY(model.key(id, Qt::Key_A, QStringLiteral("\u0001"), Qt::ControlModifier, true));
        QVERIFY(model.key(id, Qt::Key_A, QString {}, Qt::ControlModifier, false));
        QCOMPARE(commands.values.size(), 1);
        model.apply(answer(commands, { { QStringLiteral("answer"), QStringLiteral("done") } }));
        const auto queued = commands.request().value(QStringLiteral("events")).toArray();
        QCOMPARE(queued.size(), 2);
        QCOMPARE(queued.first().toObject().value(QStringLiteral("eventType")).toString(), QStringLiteral("rawKeyDown"));
        QCOMPARE(queued.first().toObject().value(QStringLiteral("key")).toString(), QStringLiteral("a"));
        QCOMPARE(queued.first().toObject().value(QStringLiteral("modifiers")).toInt(), 2);
        QVERIFY(model.key(id, Qt::Key_Eacute, QStringLiteral("é"), Qt::NoModifier, true));
        QVERIFY(!model.key(id, Qt::Key_Shift, QString {}, Qt::ShiftModifier, true));
    }

    void theBrowserPictureFollowsEachNewFrame()
    {
        Commands commands;
        kodosi::SessionCatalogModel sessions;
        kodosi::CanvasModel model(commands, sessions);
        const auto id = test::id(1);
        QSignalSpy pictures(&model, &kodosi::CanvasModel::pictureChanged);
        model.watch(id);
        QCOMPARE(commands.request().value(QStringLiteral("type")).toString(), QStringLiteral("picture"));
        QCOMPARE(commands.request().value(QStringLiteral("after")).toInt(), 0);
        QImage frame(64, 36, QImage::Format_RGB32);
        frame.fill(Qt::darkGreen);
        QByteArray jpeg;
        QBuffer buffer(&jpeg);
        buffer.open(QIODevice::WriteOnly);
        QVERIFY(frame.save(&buffer, "JPEG"));
        model.apply(answer(commands, { { QStringLiteral("answer"), QStringLiteral("picture") }, { QStringLiteral("sequence"), 7 },
                                         { QStringLiteral("frame"), QJsonObject { { QStringLiteral("width"), 32 },
                                                                        { QStringLiteral("height"), 18 },
                                                                        { QStringLiteral("data"), QString::fromLatin1(jpeg.toBase64()) } } } }));
        QCOMPARE(pictures.size(), 1);
        QCOMPARE(model.browserImage(id).size(), QSize(64, 36));
        QCOMPARE(model.picture(id).value(QStringLiteral("sequence")).toInt(), 7);
        QCOMPARE(commands.request().value(QStringLiteral("after")).toInt(), 7);
        model.setBrowserView(id, 640, 360);
        model.pointer(id, QStringLiteral("mousePressed"), 320, 180, Qt::LeftButton, 1, Qt::NoModifier);
        const auto press = commands.request().value(QStringLiteral("events")).toArray().first().toObject();
        QCOMPARE(press.value(QStringLiteral("x")).toInt(), 16);
        QCOMPARE(press.value(QStringLiteral("y")).toInt(), 9);
        QCOMPARE(press.value(QStringLiteral("button")).toString(), QStringLiteral("left"));
        model.unwatch(id);
    }

    void askAboutThisTypesOneLineForThePersonToFinish()
    {
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), {}), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral(" round\neach\u001b[2J line\r")),
            QStringLiteral("About /work/plan.md: \"round each [2J line\" "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("/work/plan.md"), QStringLiteral("  ")), QStringLiteral("About /work/plan.md: "));
        QCOMPARE(kodosi::CanvasModel::about(QStringLiteral("x"), QString(700, QLatin1Char('w'))).size(), 612);
    }

    void aPageLinkOpensOnThisDeviceButNotALocalAddressOfAnotherComputer()
    {
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("https://kodosi.com/docs")), true));
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("mailto:ana@example.com")), true));
        QVERIFY(kodosi::CanvasModel::followable(QUrl(QStringLiteral("http://localhost:3000")), false));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("http://localhost:3000")), true));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("http://[::1]:3000")), true));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("http://app.localhost")), true));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("file:///etc/passwd")), false));
        QVERIFY(!kodosi::CanvasModel::followable(QUrl(QStringLiteral("javascript:alert(1)")), false));
    }

    void theOpenCanvasesFollowTheirTerminals()
    {
        QTemporaryDir directory;
        kodosi::SessionCatalogModel sessions;
        kodosi::DesktopStateModel desktop(
            std::make_unique<QSettings>(directory.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat));
        desktop.attachSessionCatalog(&sessions);
        const auto id = test::id(1);
        sessions.apply(test::snapshot({ terminal(canvas(1, { item(QStringLiteral("plan.md"), 1) })) }));
        desktop.toggleCanvas(id);
        QCOMPARE(desktop.openCanvases(), QStringList { id });
        QCOMPARE(desktop.selectedSessionId(), id);
        sessions.apply(test::snapshot({ terminal({}) }));
        QVERIFY(desktop.openCanvases().isEmpty());
        sessions.apply(test::snapshot({ terminal(canvas(2, { item(QStringLiteral("plan.md"), 2) })) }));
        desktop.toggleCanvas(id);
        QVERIFY(desktop.unstageSession(id));
        QVERIFY(desktop.openCanvases().isEmpty());
    }
};

QTEST_MAIN(CanvasModelTest)
#include "tst_canvas_model.moc"
