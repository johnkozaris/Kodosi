#include "platform/TerminalNotifications.hpp"

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include "SessionFixture.hpp"
#include <QSignalSpy>
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

    void invoke(
        const QString& key,
        const QString& action,
        const QString& activationToken = {})
    {
        emit actionInvoked(key, action, activationToken);
    }

    void close(const QString& key)
    {
        emit notificationClosed(key);
    }
};

class TerminalNotificationsTest final : public QObject {
    Q_OBJECT

private slots:
    void postsAndActivatesCurrentSession();
    void ignoresRemoteOrMissingIncarnationEvents();
    void rejectsStaleSessionIncarnations();
    void routesOnlyTerminalNotificationKeys();
    void boundsTrackedNotifications();
    void alertsTheOwnerAndThePeopleWhoHaveAWaitingTerminalOpen();
};

namespace {

QJsonObject sessions(const QString& incarnation)
{
    auto entry=test::session(1);
    entry.insert(QStringLiteral("incarnationId"),incarnation);
    return test::snapshot({entry});
}

struct Fixture {
    kodosi::SessionCatalogModel sessionsModel;
    FakeTerminalNotificationDriver driver;
    kodosi::TerminalNotifications notifications {
        sessionsModel,
        driver,
    };

    Fixture()
    {
        sessionsModel.apply(
            sessions(test::id(101)));
    }

    void post(
        const QString& incarnation = test::id(101),
        const QString& title = QStringLiteral("Build complete"),
        const QString& body = QStringLiteral("All checks passed"))
    {
        notifications.apply({
            {QStringLiteral("type"), QStringLiteral("term.notification")},
            {QStringLiteral("sessionId"), test::id(1)},
            {QStringLiteral("runtimeIncarnationId"), incarnation},
            {QStringLiteral("title"), title},
            {QStringLiteral("body"), body},
        });
    }
};

}

void TerminalNotificationsTest::postsAndActivatesCurrentSession()
{
    Fixture fixture;
    QSignalSpy requests(
        &fixture.notifications,
        &kodosi::TerminalNotifications::sessionRequested);
    fixture.post();

    QCOMPARE(fixture.driver.posted.size(), 1);
    const auto notification = fixture.driver.posted.constFirst();
    QVERIFY(notification.key.startsWith(QStringLiteral("terminal:")));
    QCOMPARE(notification.title, QStringLiteral("Build complete"));
    QCOMPARE(notification.body, QStringLiteral("All checks passed"));
    QCOMPARE(notification.destination.sessionId, test::id(1));

    fixture.driver.invoke(
        notification.key,
        QStringLiteral("default"),
        QStringLiteral("activation-token"));
    QCOMPARE(requests.size(), 1);
    QCOMPARE(
        requests.constFirst().at(0).toString(),
        test::id(1));
    QCOMPARE(
        requests.constFirst().at(1).toString(),
        QStringLiteral("activation-token"));
    QCOMPARE(fixture.driver.withdrawn.constLast(), notification.key);
}

void TerminalNotificationsTest::rejectsStaleSessionIncarnations()
{
    Fixture fixture;
    fixture.post(QStringLiteral("stale"));
    QVERIFY(fixture.driver.posted.isEmpty());

    fixture.post();
    const auto key = fixture.driver.posted.constFirst().key;
    fixture.sessionsModel.apply(
        sessions(test::id(102)));
    QVERIFY(fixture.driver.withdrawn.contains(key));

    QSignalSpy requests(
        &fixture.notifications,
        &kodosi::TerminalNotifications::sessionRequested);
    fixture.driver.invoke(
        key,
        QStringLiteral("default"));
    QVERIFY(requests.isEmpty());
}

void TerminalNotificationsTest::routesOnlyTerminalNotificationKeys()
{
    Fixture fixture;
    fixture.post();
    const auto key = fixture.driver.posted.constFirst().key;
    fixture.driver.invoke(
        QStringLiteral("unrelated:opaque"),
        QStringLiteral("default"));
    fixture.driver.invoke(key, QStringLiteral("unknown-action"));
    QVERIFY(fixture.driver.withdrawn.isEmpty());
}

void TerminalNotificationsTest::boundsTrackedNotifications()
{
    Fixture fixture;
    for (auto index = 0; index < 257; ++index) {
        fixture.post(
            test::id(101),
            QStringLiteral("Terminal"),
            QString::number(index));
    }
    QCOMPARE(fixture.driver.posted.size(), 257);
    QCOMPARE(fixture.driver.withdrawn.size(), 1);
    QCOMPARE(
        fixture.driver.withdrawn.constFirst(),
        fixture.driver.posted.constFirst().key);

    fixture.driver.close(fixture.driver.posted.at(1).key);
    fixture.post();
    QCOMPARE(fixture.driver.withdrawn.size(), 1);
}

void TerminalNotificationsTest::ignoresRemoteOrMissingIncarnationEvents()
{
    Fixture fixture;
    fixture.post(QString {});
    QVERIFY(fixture.driver.posted.isEmpty());
    fixture.sessionsModel.apply(test::snapshot({test::session(1, true)}));
    fixture.post();
    QVERIFY(fixture.driver.posted.isEmpty());
    fixture.sessionsModel.apply(test::snapshot({test::session(1)}));
    fixture.post();
    QCOMPARE(fixture.driver.posted.size(), 1);
}

void TerminalNotificationsTest::alertsTheOwnerAndThePeopleWhoHaveAWaitingTerminalOpen()
{
    Fixture fixture;
    fixture.notifications.isFront = [] { return false; };
    const QStringList all {test::id(1), test::id(2), test::id(3)};
    const auto apply = [&](const QJsonObject& status, const QStringList& marked) {
        QJsonArray entries;
        for (int number = 1; number <= 3; ++number) {
            auto entry = test::session(number, number > 1, number == 1);
            entry.insert(QStringLiteral("programStatus"), status);
            entry.insert(QStringLiteral("attention"), marked.contains(test::id(number)));
            if (number == 3)
                entry.insert(QStringLiteral("connectionState"), QStringLiteral("offline"));
            entries.append(entry);
        }
        fixture.sessionsModel.apply(test::snapshot(entries));
    };
    const auto alert = [&] {
        for (const auto& id : all)
            fixture.sessionsModel.alert(id);
    };
    const QJsonObject works {{QStringLiteral("state"), QStringLiteral("working")}};
    apply(works, all);
    alert();
    QVERIFY(fixture.driver.posted.isEmpty());
    apply({{QStringLiteral("state"), QStringLiteral("blocked")}, {QStringLiteral("kind"), QStringLiteral("permission")},
        {QStringLiteral("message"), QStringLiteral("Allow the command?")}}, all);
    alert();
    QCOMPARE(fixture.driver.posted.size(), 2);
    for (const auto& posted : std::as_const(fixture.driver.posted)) {
        QCOMPARE(posted.title, QStringLiteral("Terminal"));
        QCOMPARE(posted.body, QStringLiteral("Allow the command?"));
    }
    QCOMPARE(fixture.driver.posted.at(0).destination.sessionId, test::id(1));
    QCOMPARE(fixture.driver.posted.at(1).destination.sessionId, test::id(2));

    apply(works, all);
    QVERIFY(fixture.driver.withdrawn.contains(fixture.driver.posted.at(0).key));
    QVERIFY(fixture.driver.withdrawn.contains(fixture.driver.posted.at(1).key));
    apply({{QStringLiteral("state"), QStringLiteral("done")}}, all);
    alert();
    QCOMPARE(fixture.driver.posted.size(), 4);
    QCOMPARE(fixture.driver.posted.at(2).body, QStringLiteral("Done"));
    QCOMPARE(fixture.driver.posted.at(3).destination.sessionId, test::id(2));
    const auto withdrawn = fixture.driver.withdrawn.size();
    apply({{QStringLiteral("state"), QStringLiteral("done")}}, {test::id(2), test::id(3)});
    QCOMPARE(fixture.driver.withdrawn.size(), withdrawn + 1);

    fixture.notifications.isFront = [] { return true; };
    apply({{QStringLiteral("state"), QStringLiteral("error")}}, all);
    alert();
    QCOMPARE(fixture.driver.posted.size(), 4);
}

QTEST_GUILESS_MAIN(TerminalNotificationsTest)

#include "tst_terminal_notifications.moc"
