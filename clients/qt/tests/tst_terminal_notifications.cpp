#include "platform/TerminalNotifications.hpp"

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include "SessionFixture.hpp"
#include <QtTest/QTest>

class FakeTerminalNotificationDriver final
    : public kodosi::DesktopNotificationDriver {
public:
    using DesktopNotificationDriver::DesktopNotificationDriver;

    QVector<kodosi::DesktopNotification> posted;
    QVector<QString> withdrawn;

    void post(const kodosi::DesktopNotification& notification) override
    {
        posted.push_back(notification);
    }

    void withdraw(const QString& key) override
    {
        withdrawn.push_back(key);
    }
};

class TerminalNotificationsTest final : public QObject {
    Q_OBJECT

private slots:
    void aCallIsPostedAsItsCueSaysAndReplacedPerTerminal();
    void aCallLeavesWhenItsMarkOrWaitEndsOrTheTerminalGoes();
    void aProgramNotificationLeavesWhenTheMarkOfItsTerminalEnds();
    void ignoresRemoteOrStaleProgramNotifications();
};

namespace {

QJsonObject waiting(int number, const QString& state, bool marked, const QString& message = {})
{
    auto entry = test::session(number);
    entry.insert(QStringLiteral("waiting"), state);
    entry.insert(QStringLiteral("attention"), marked);
    QJsonObject status { { QStringLiteral("state"), state } };
    if (!message.isEmpty())
        status.insert(QStringLiteral("message"), message);
    entry.insert(QStringLiteral("programStatus"), status);
    return entry;
}

QJsonObject marked(QJsonObject entry)
{
    entry.insert(QStringLiteral("attention"), true);
    return entry;
}

QJsonObject attention(int number, const QString& cue = QStringLiteral("sound"))
{
    return { { QStringLiteral("type"), QStringLiteral("term.attention") },
        { QStringLiteral("sessionId"), test::id(number) }, { QStringLiteral("cue"), cue } };
}

QJsonObject programNotification(const QString& incarnation, const QString& body)
{
    return { { QStringLiteral("type"), QStringLiteral("term.notification") },
        { QStringLiteral("sessionId"), test::id(1) }, { QStringLiteral("runtimeIncarnationId"), incarnation },
        { QStringLiteral("title"), QStringLiteral("Build") }, { QStringLiteral("body"), body },
        { QStringLiteral("cue"), QStringLiteral("list") } };
}

struct Fixture {
    kodosi::SessionCatalogModel sessions;
    FakeTerminalNotificationDriver driver;
    kodosi::TerminalNotifications notifications { sessions, driver };
};

}

void TerminalNotificationsTest::aCallIsPostedAsItsCueSaysAndReplacedPerTerminal()
{
    Fixture fixture;
    fixture.sessions.apply(test::snapshot({ waiting(1, QStringLiteral("blocked"), true, QStringLiteral("Allow the command?")),
        waiting(2, QStringLiteral("done"), true) }));

    fixture.notifications.apply(attention(1, QStringLiteral("sound")));
    fixture.notifications.apply(attention(2, QStringLiteral("banner")));
    QCOMPARE(fixture.driver.posted.size(), 2);
    const auto first = fixture.driver.posted.at(0);
    QCOMPARE(first.sessionId, test::id(1));
    QCOMPARE(first.title, QStringLiteral("Terminal"));
    QCOMPARE(first.body, QStringLiteral("Allow the command?"));
    QVERIFY(first.cue == kodosi::NotificationCue::Sound);
    const auto second = fixture.driver.posted.at(1);
    QCOMPARE(second.body, QStringLiteral("Done"));
    QVERIFY(second.cue == kodosi::NotificationCue::Banner);
    QVERIFY(first.key != second.key);

    fixture.notifications.apply(attention(1, QStringLiteral("list")));
    QCOMPARE(fixture.driver.posted.size(), 3);
    QCOMPARE(fixture.driver.posted.at(2).key, first.key);
    QVERIFY(fixture.driver.posted.at(2).cue == kodosi::NotificationCue::List);
    QVERIFY(fixture.driver.withdrawn.isEmpty());

    fixture.notifications.apply(attention(1, QStringLiteral("loud")));
    fixture.notifications.apply(attention(1, {}));
    QCOMPARE(fixture.driver.posted.size(), 3);
}

void TerminalNotificationsTest::aCallLeavesWhenItsMarkOrWaitEndsOrTheTerminalGoes()
{
    Fixture fixture;
    const auto blocked = [](bool one, bool two, bool three) {
        return test::snapshot({ waiting(1, QStringLiteral("blocked"), one), waiting(2, QStringLiteral("blocked"), two),
            waiting(3, QStringLiteral("blocked"), three) });
    };
    fixture.sessions.apply(blocked(true, true, true));
    for (const int number : { 1, 2, 3 })
        fixture.notifications.apply(attention(number));
    QCOMPARE(fixture.driver.posted.size(), 3);

    fixture.sessions.apply(blocked(false, true, true));
    QCOMPARE(fixture.driver.withdrawn, QVector<QString> { fixture.driver.posted.at(0).key });

    auto working = test::session(2);
    working.insert(QStringLiteral("working"), true);
    working.insert(QStringLiteral("attention"), true);
    fixture.sessions.apply(test::snapshot({ working, waiting(3, QStringLiteral("blocked"), true) }));
    QVERIFY(fixture.driver.withdrawn.contains(fixture.driver.posted.at(1).key));

    fixture.sessions.apply(test::snapshot({}));
    QVERIFY(fixture.driver.withdrawn.contains(fixture.driver.posted.at(2).key));

    fixture.sessions.apply(blocked(true, false, false));
    fixture.notifications.apply(attention(1));
    QCOMPARE(fixture.driver.posted.size(), 4);
    QCOMPARE(fixture.driver.posted.constLast().key, fixture.driver.posted.at(0).key);
}

void TerminalNotificationsTest::aProgramNotificationLeavesWhenTheMarkOfItsTerminalEnds()
{
    Fixture fixture;
    fixture.sessions.apply(test::snapshot({ test::session(1) }));
    fixture.notifications.apply(programNotification(test::id(101), QStringLiteral("Unmarked")));
    QVERIFY(fixture.driver.posted.isEmpty());

    fixture.sessions.apply(test::snapshot({ marked(test::session(1)) }));
    fixture.notifications.apply(programNotification(test::id(101), QStringLiteral("Finished")));
    QCOMPARE(fixture.driver.posted.size(), 1);
    fixture.sessions.apply(test::snapshot({ marked(test::session(1)) }));
    QVERIFY(fixture.driver.withdrawn.isEmpty());
    fixture.sessions.apply(test::snapshot({ test::session(1) }));
    QCOMPARE(fixture.driver.withdrawn, QVector<QString> { fixture.driver.posted.constFirst().key });
}

void TerminalNotificationsTest::ignoresRemoteOrStaleProgramNotifications()
{
    Fixture fixture;
    fixture.sessions.apply(test::snapshot({ marked(test::session(1, true)) }));
    fixture.notifications.apply(programNotification(test::id(101), QStringLiteral("Remote")));
    QVERIFY(fixture.driver.posted.isEmpty());

    fixture.sessions.apply(test::snapshot({ marked(test::session(1)) }));
    fixture.notifications.apply(programNotification(QStringLiteral("stale"), QStringLiteral("Old")));
    QVERIFY(fixture.driver.posted.isEmpty());
    fixture.notifications.apply(programNotification(test::id(101), QStringLiteral("Current")));
    QCOMPARE(fixture.driver.posted.size(), 1);

    auto restarted = marked(test::session(1));
    restarted.insert(QStringLiteral("incarnationId"), test::id(102));
    fixture.sessions.apply(test::snapshot({ restarted }));
    QCOMPARE(fixture.driver.withdrawn, QVector<QString> { fixture.driver.posted.constFirst().key });
}

QTEST_GUILESS_MAIN(TerminalNotificationsTest)

#include "tst_terminal_notifications.moc"
