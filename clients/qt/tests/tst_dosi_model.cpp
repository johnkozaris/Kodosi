#include "SessionFixture.hpp"
#include "presentation/DesktopSettings.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/DosiWords.hpp"
#include "presentation/Workspace.hpp"
#include "runtime/RuntimeBridge.hpp"
#include <QJsonArray>
#include <QJsonDocument>
#include <QTemporaryDir>
#include <QtTest/QTest>

class Commands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> values;
    Result send(QByteArrayView bytes) override
    {
        values.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
};

struct Fixture {
    QTemporaryDir dir;
    Commands commands;
    kodosi::SessionCatalogModel sessions;
    kodosi::DesktopStateModel desktop {
        std::make_unique<QSettings>(dir.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat)
    };
    kodosi::DesktopSettings settings {
        std::make_unique<QSettings>(dir.filePath(QStringLiteral("terminal.ini")), QSettings::IniFormat)
    };
    kodosi::Workspace workspace { commands, sessions, desktop, settings };
    kodosi::DosiModel& dosi = workspace.dosi();

    Fixture() { desktop.attachSessionCatalog(&sessions); }

    void snapshot(const QJsonArray& items, const QString& conversation = QStringLiteral("c1"), bool earlier = false,
        const QString& state = QStringLiteral("idle"))
    {
        workspace.apply({ { QStringLiteral("type"), QStringLiteral("dosi.snapshot") },
                            { QStringLiteral("dosi"), QJsonObject { { QStringLiteral("state"), state },
                                { QStringLiteral("conversation"), QJsonObject { { QStringLiteral("id"), conversation } } },
                                { QStringLiteral("items"), items }, { QStringLiteral("earlier"), earlier } } } },
            0);
    }
    void item(const QJsonObject& value)
    {
        workspace.apply({ { QStringLiteral("type"), QStringLiteral("dosi.item") }, { QStringLiteral("item"), value } }, 0);
    }
    QStringList ids() const
    {
        QStringList result;
        for (const auto& row : dosi.rows())
            result.append(row.toMap().value(QStringLiteral("id")).toString());
        return result;
    }
};

namespace {
QJsonObject entry(const QString& id, const QJsonObject& body, const QString& turn = QStringLiteral("t1"), qint64 at = 1792000000000)
{
    return { { QStringLiteral("id"), id }, { QStringLiteral("turn"), turn }, { QStringLiteral("at"), at }, { QStringLiteral("body"), body } };
}
QJsonObject words(const QString& kind, const QString& text)
{
    return { { QStringLiteral("kind"), kind }, { QStringLiteral("text"), text }, { QStringLiteral("live"), false } };
}
QJsonObject tool(const QString& action, const QString& kind, const QString& value, const QString& status,
    const QString& text = {})
{
    QJsonObject result { { QStringLiteral("kind"), QStringLiteral("tool") }, { QStringLiteral("action"), action },
        { QStringLiteral("target"), QJsonObject { { QStringLiteral("kind"), kind }, { QStringLiteral("value"), value } } },
        { QStringLiteral("status"), status } };
    if (!text.isEmpty())
        result.insert(QStringLiteral("text"), text);
    return result;
}
}

class DosiModelTest final : public QObject {
    Q_OBJECT
private slots:
    void aSnapshotKeepsEarlierPagesAndANewConversationStartsClean()
    {
        Fixture f;
        f.snapshot({ entry(QStringLiteral("c"), words(QStringLiteral("person"), QStringLiteral("Hi"))) }, QStringLiteral("c1"), true);
        f.dosi.loadEarlier();
        const auto history = f.commands.values.last();
        QCOMPARE(history.value(QStringLiteral("type")).toString(), QStringLiteral("dosi.history"));
        QCOMPARE(history.value(QStringLiteral("before")).toString(), QStringLiteral("c"));
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("dosi.result") },
                              { QStringLiteral("requestId"), history.value(QStringLiteral("requestId")) },
                              { QStringLiteral("operation"), QStringLiteral("dosi.history") },
                              { QStringLiteral("page"), QJsonObject { { QStringLiteral("earlier"), false },
                                  { QStringLiteral("items"), QJsonArray { entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("Old"))),
                                      entry(QStringLiteral("c"), words(QStringLiteral("person"), QStringLiteral("Hi"))) } } } } },
            0);
        QCOMPARE(f.ids(), (QStringList { QStringLiteral("divider.a"), QStringLiteral("a"), QStringLiteral("c") }));
        QCOMPARE(f.dosi.details().value(QStringLiteral("earlier")).toBool(), false);
        f.snapshot({ entry(QStringLiteral("c"), words(QStringLiteral("person"), QStringLiteral("Hi"))),
            entry(QStringLiteral("d"), words(QStringLiteral("text"), QStringLiteral("Hello"))) });
        QCOMPARE(f.ids(), (QStringList { QStringLiteral("divider.a"), QStringLiteral("a"), QStringLiteral("c"), QStringLiteral("d") }));
        f.snapshot({}, QStringLiteral("c2"));
        QVERIFY(f.dosi.rows().isEmpty());
        QVERIFY(f.dosi.empty());
    }

    void theThreadGroupsStepsOfATurnAndMarksLongGaps()
    {
        Fixture f;
        const auto now = QDateTime(QDate(2026, 10, 18), QTime(15, 0));
        const auto at = [&](int minutes) { return now.addSecs(minutes * 60).toMSecsSinceEpoch(); };
        f.snapshot({ entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("What needs me?")), QStringLiteral("1"), at(-1500)),
            entry(QStringLiteral("b"), tool(QStringLiteral("look"), QStringLiteral("terminals"), {}, QStringLiteral("done")), QStringLiteral("1"), at(-1499)),
            entry(QStringLiteral("c"), tool(QStringLiteral("read"), QStringLiteral("terminal"), QStringLiteral("api"), QStringLiteral("done")), QStringLiteral("1"), at(-1499)),
            entry(QStringLiteral("d"), words(QStringLiteral("text"), QStringLiteral("Two need you.")), QStringLiteral("1"), at(-1498)),
            entry(QStringLiteral("x"), words(QStringLiteral("person"), QStringLiteral("Anything new?")), QStringLiteral("x"), at(-120)),
            entry(QStringLiteral("e"), words(QStringLiteral("person"), QStringLiteral("Run the tests.")), QStringLiteral("2"), at(-5)),
            entry(QStringLiteral("f"), tool(QStringLiteral("type"), QStringLiteral("terminal"), QStringLiteral("build"), QStringLiteral("done"), QStringLiteral("just test")),
                QStringLiteral("2"), at(-4)) });
        const auto rows = f.dosi.buildRows(now);
        QStringList ids;
        for (const auto& row : rows)
            ids.append(row.toMap().value(QStringLiteral("id")).toString());
        QCOMPARE(ids, (QStringList { QStringLiteral("divider.a"), QStringLiteral("a"), QStringLiteral("b"), QStringLiteral("d"),
                          QStringLiteral("divider.x"), QStringLiteral("x"), QStringLiteral("e"), QStringLiteral("f") }));
        QCOMPARE(rows.at(0).toMap().value(QStringLiteral("label")).toString(), QStringLiteral("yesterday"));
        QCOMPARE(rows.at(4).toMap().value(QStringLiteral("label")).toString(), QStringLiteral("this afternoon"));
        const auto steps = rows.at(2).toMap();
        QCOMPARE(steps.value(QStringLiteral("kind")).toString(), QStringLiteral("tools"));
        QCOMPARE(steps.value(QStringLiteral("summary")).toString(), QStringLiteral("worked on the terminals, api"));
        QVERIFY(steps.value(QStringLiteral("starts")).toBool());
        QVERIFY(!rows.at(3).toMap().value(QStringLiteral("starts")).toBool());
        QCOMPARE(rows.at(7).toMap().value(QStringLiteral("tool")).toMap().value(QStringLiteral("receipt")).toString(),
            QStringLiteral("typed just test · build"));
        const auto typing = tool(QStringLiteral("type"), QStringLiteral("terminal"), QStringLiteral("build"), QStringLiteral("running"),
            QStringLiteral("just test")).toVariantMap();
        QCOMPARE(kodosi::DosiWords::receipt(typing), QStringLiteral("typing just test · build"));
        QCOMPARE(kodosi::DosiWords::said(typing), QStringLiteral("typing just test"));
    }

    void aStepInATerminalTouchesItOnceAndTheCardFollowsTheStep()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        const auto id = test::id(1);
        const auto touch = [&] { return f.dosi.touches().value(id).toMap(); };
        f.item(entry(QStringLiteral("a"), tool(QStringLiteral("type"), QStringLiteral("terminal"), QStringLiteral("terminal"), QStringLiteral("running"), QStringLiteral("y"))));
        f.item(entry(QStringLiteral("a"), tool(QStringLiteral("type"), QStringLiteral("terminal"), QStringLiteral("terminal"), QStringLiteral("done"), QStringLiteral("y"))));
        QCOMPARE(touch().value(QStringLiteral("count")).toInt(), 1);
        QCOMPARE(touch().value(QStringLiteral("said")).toString(), QStringLiteral("typed y"));
        QVERIFY(touch().value(QStringLiteral("shows")).toBool());
        f.item(entry(QStringLiteral("b"), tool(QStringLiteral("read"), QStringLiteral("terminal"), QStringLiteral("terminal"), QStringLiteral("running"))));
        f.item(entry(QStringLiteral("a"), tool(QStringLiteral("type"), QStringLiteral("terminal"), QStringLiteral("terminal"), QStringLiteral("done"), QStringLiteral("y"))));
        QCOMPARE(touch().value(QStringLiteral("count")).toInt(), 2);
        QVERIFY(!touch().value(QStringLiteral("shows")).toBool());
    }

    void aMessageCarriesTheOpenTerminalAndAFailureGivesItsTextBack()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        f.desktop.selectSession(test::id(1));
        f.snapshot({});
        f.dosi.setDraft(QStringLiteral("  Check the build  "));
        f.dosi.send();
        const auto sent = f.commands.values.last();
        QCOMPARE(sent.value(QStringLiteral("type")).toString(), QStringLiteral("dosi.send"));
        QCOMPARE(sent.value(QStringLiteral("text")).toString(), QStringLiteral("Check the build"));
        QCOMPARE(sent.value(QStringLiteral("context")).toObject().value(QStringLiteral("terminal")).toString(), test::id(1));
        QVERIFY(f.dosi.draft().isEmpty());
        f.workspace.apply({ { QStringLiteral("type"), QStringLiteral("dosi.error") }, { QStringLiteral("requestId"), sent.value(QStringLiteral("requestId")) },
                              { QStringLiteral("operation"), QStringLiteral("dosi.send") }, { QStringLiteral("message"), QStringLiteral("Dosi is not set up.") } },
            0);
        QCOMPARE(f.dosi.draft(), QStringLiteral("Check the build"));
        QCOMPARE(f.workspace.error(), QStringLiteral("Dosi is not set up."));
    }

    void aQuestionAboutATerminalWaitsOnItsTileUntilItIsAnswered()
    {
        Fixture f;
        f.workspace.apply(test::snapshot({ test::session(1) }), 0);
        QJsonObject question { { QStringLiteral("kind"), QStringLiteral("question") }, { QStringLiteral("question"), QStringLiteral("Ship it?") },
            { QStringLiteral("options"), QJsonArray { QStringLiteral("y"), QStringLiteral("n") } }, { QStringLiteral("terminal"), QStringLiteral("Terminal") } };
        f.snapshot({ entry(QStringLiteral("q"), question) });
        QVERIFY(f.dosi.asking());
        QCOMPARE(f.dosi.terminalQuestions().value(test::id(1)).toMap().value(QStringLiteral("item")).toString(), QStringLiteral("q"));
        QVERIFY(f.dosi.rows().last().toMap().value(QStringLiteral("keys")).toBool());
        question.insert(QStringLiteral("answer"), QJsonArray { QStringLiteral("y") });
        f.item(entry(QStringLiteral("q"), question));
        QVERIFY(!f.dosi.asking());
        QVERIFY(f.dosi.terminalQuestions().isEmpty());
        QCOMPARE(f.dosi.rows().last().toMap().value(QStringLiteral("answered")).toString(), QStringLiteral("y · Terminal"));
    }

    void dosiThinksInTheThreadUntilItsTurnShowsSomething()
    {
        Fixture f;
        const auto asked = entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("What needs me?")));
        f.snapshot({ asked }, QStringLiteral("c1"), false, QStringLiteral("working"));
        QVERIFY(f.dosi.pending());
        auto reply = words(QStringLiteral("text"), QStringLiteral("Two"));
        reply.insert(QStringLiteral("live"), true);
        f.item(entry(QStringLiteral("b"), reply));
        QVERIFY(!f.dosi.pending());
        f.item(entry(QStringLiteral("b"), words(QStringLiteral("text"), QStringLiteral("Two need you."))));
        QVERIFY(f.dosi.pending());
        f.item(entry(QStringLiteral("c"), tool(QStringLiteral("read"), QStringLiteral("terminal"), QStringLiteral("api"), QStringLiteral("running"))));
        QVERIFY(!f.dosi.pending());
        f.item(entry(QStringLiteral("q"), QJsonObject { { QStringLiteral("kind"), QStringLiteral("question") },
                                              { QStringLiteral("question"), QStringLiteral("Ship it?") },
                                              { QStringLiteral("options"), QJsonArray { QStringLiteral("y"), QStringLiteral("n") } } }));
        QVERIFY(!f.dosi.pending());
        f.snapshot({ asked });
        QVERIFY(!f.dosi.pending());
    }

    void anAnswerIsNewUntilDosiIsOpen()
    {
        Fixture f;
        f.snapshot({ entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("Hi"))) }, QStringLiteral("c1"), false, QStringLiteral("working"));
        QVERIFY(!f.dosi.unseen());
        f.snapshot({ entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("Hi"))),
            entry(QStringLiteral("b"), words(QStringLiteral("text"), QStringLiteral("Hello"))) });
        QVERIFY(f.dosi.unseen());
        f.desktop.openDosi();
        QVERIFY(!f.dosi.unseen());
        f.desktop.setDosiOpen(false);
        f.item(entry(QStringLiteral("c"), words(QStringLiteral("text"), QStringLiteral("More")), QStringLiteral("t2")));
        QVERIFY(f.dosi.unseen());
    }

    void aReplyFromBeforeTheStartIsReadAndANewOneIsNot()
    {
        Fixture f;
        f.snapshot({ entry(QStringLiteral("a"), words(QStringLiteral("person"), QStringLiteral("Hi"))),
            entry(QStringLiteral("b"), words(QStringLiteral("text"), QStringLiteral("Hello."))) });
        QVERIFY(!f.dosi.unseen());
        QCOMPARE(f.dosi.line(), QStringLiteral("ask dosi"));
        f.item(entry(QStringLiteral("c"), words(QStringLiteral("person"), QStringLiteral("And?")), QStringLiteral("t2")));
        QVERIFY(!f.dosi.unseen());
        f.item(entry(QStringLiteral("d"), words(QStringLiteral("text"), QStringLiteral("Done.")), QStringLiteral("t2")));
        QVERIFY(f.dosi.unseen());
        QCOMPARE(f.dosi.line(), QStringLiteral("dosi answered"));
    }

    void turningDosiOffClosesItAndKeepsItClosed()
    {
        Fixture f;
        f.desktop.openDosi();
        f.desktop.setDosiOn(false);
        QVERIFY(!f.desktop.dosiOpen());
        f.desktop.toggleDosi();
        QVERIFY(!f.desktop.dosiOpen());
        kodosi::DesktopStateModel restored { std::make_unique<QSettings>(f.dir.filePath(QStringLiteral("ui.ini")), QSettings::IniFormat) };
        QVERIFY(!restored.dosiOn() && !restored.dosiOpen());
    }
};

QTEST_GUILESS_MAIN(DosiModelTest)
#include "tst_dosi_model.moc"
