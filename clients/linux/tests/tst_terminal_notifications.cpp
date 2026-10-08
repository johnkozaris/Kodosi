#include "platform/TerminalNotifications.hpp"

#include <QByteArray>
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

QTEST_GUILESS_MAIN(TerminalNotificationsTest)

#include "tst_terminal_notifications.moc"
