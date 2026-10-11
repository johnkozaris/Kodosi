#include "platform/FreedesktopNotificationDriver.hpp"

#include <QDBusConnection>
#include <QDBusPendingCallWatcher>
#include <QSignalSpy>
#include <QtTest/QTest>

class FakeFreedesktopNotificationService final : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.Notifications")

public:
    struct Notification {
        QString appName;
        uint replacesId;
        QString appIcon;
        QString summary;
        QString body;
        QStringList actions;
        QVariantMap hints;
        int timeout;
    };

    QVector<Notification> notifications;
    QVector<uint> closed;

public slots:
    uint Notify(
        const QString& appName,
        const uint replacesId,
        const QString& appIcon,
        const QString& summary,
        const QString& body,
        const QStringList& actions,
        const QVariantMap& hints,
        const int timeout)
    {
        notifications.push_back({
            .appName = appName,
            .replacesId = replacesId,
            .appIcon = appIcon,
            .summary = summary,
            .body = body,
            .actions = actions,
            .hints = hints,
            .timeout = timeout,
        });
        return 42;
    }

    void CloseNotification(const uint id)
    {
        closed.push_back(id);
        emit NotificationClosed(id, 3);
    }

signals:
    void ActionInvoked(uint id, QString action);
    void ActivationToken(uint id, QString token);
    void NotificationClosed(uint id, uint reason);
};

class FreedesktopNotificationDriverTest final : public QObject {
    Q_OBJECT

private slots:
    void initTestCase();
    void cleanupTestCase();
    void postsHandlesActionsAndWithdraws();
    void pendingPostsRemainBoundedAfterWithdrawal();

private:
    QDBusConnection m_bus = QDBusConnection::sessionBus();
    FakeFreedesktopNotificationService m_service;
};

void FreedesktopNotificationDriverTest::initTestCase()
{
    QVERIFY(m_bus.isConnected());
    QVERIFY(m_bus.registerService(
        QStringLiteral("org.freedesktop.Notifications")));
    QVERIFY(m_bus.registerObject(
        QStringLiteral("/org/freedesktop/Notifications"),
        &m_service,
        QDBusConnection::ExportAllSlots
            | QDBusConnection::ExportAllSignals));
}

void FreedesktopNotificationDriverTest::cleanupTestCase()
{
    m_bus.unregisterObject(
        QStringLiteral("/org/freedesktop/Notifications"));
    m_bus.unregisterService(
        QStringLiteral("org.freedesktop.Notifications"));
}

void FreedesktopNotificationDriverTest::postsHandlesActionsAndWithdraws()
{
    kodosi::FreedesktopNotificationDriver driver;
    QSignalSpy actions(
        &driver,
        &kodosi::DesktopNotificationDriver::actionInvoked);
    QSignalSpy errors(
        &driver,
        &kodosi::DesktopNotificationDriver::deliveryError);

    driver.post({
        .key = QStringLiteral("request-identity"),
        .title = QStringLiteral("Bash <Session>"),
        .body = QStringLiteral("<b>plain text</b>"),
    });

    QTRY_COMPARE(m_service.notifications.size(), 1);
    QVERIFY(errors.isEmpty());
    const auto& notification = m_service.notifications.constFirst();
    QCOMPARE(notification.appName, QStringLiteral("Kodosi"));
    QCOMPARE(notification.replacesId, uint {0});
    QCOMPARE(
        notification.appIcon,
        QStringLiteral("com.kodosi.Kodosi"));
    QCOMPARE(
        notification.summary,
        QStringLiteral("Bash &lt;Session&gt;"));
    QCOMPARE(
        notification.body,
        QStringLiteral("&lt;b&gt;plain text&lt;/b&gt;"));
    QCOMPARE(
        notification.actions,
        QStringList({
            QStringLiteral("default"),
            QStringLiteral("Open Kodosi"),
        }));
    QCOMPARE(
        notification.hints.value(QStringLiteral("desktop-entry")).toString(),
        QStringLiteral("com.kodosi.Kodosi"));
    QCOMPARE(notification.timeout, -1);

    QTest::qWait(20);
    emit m_service.ActivationToken(
        42,
        QStringLiteral("activation-token"));
    emit m_service.ActionInvoked(
        42,
        QStringLiteral("default"));
    QTRY_COMPARE(actions.size(), 1);
    QCOMPARE(
        actions.constFirst().at(0).toString(),
        QStringLiteral("request-identity"));
    QCOMPARE(
        actions.constFirst().at(1).toString(),
        QStringLiteral("default"));
    QCOMPARE(
        actions.constFirst().at(2).toString(),
        QStringLiteral("activation-token"));

    driver.withdraw(QStringLiteral("request-identity"));
    QTRY_COMPARE(m_service.closed, QVector<uint> {42});
}

void FreedesktopNotificationDriverTest::pendingPostsRemainBoundedAfterWithdrawal()
{
    kodosi::FreedesktopNotificationDriver driver;
    QSignalSpy errors(&driver, &kodosi::DesktopNotificationDriver::deliveryError);
    for (int i = 0; i < 1000; ++i) {
        const auto key = QString::number(i);
        driver.post({key, QStringLiteral("Terminal"), QStringLiteral("Complete")});
        driver.withdraw(key);
    }
    QCOMPARE(errors.count(), 968);
    QVERIFY(driver.findChildren<QDBusPendingCallWatcher*>().size() <= 32);
    QTRY_VERIFY(driver.findChildren<QDBusPendingCallWatcher*>().isEmpty());
}

QTEST_GUILESS_MAIN(FreedesktopNotificationDriverTest)

#include "tst_freedesktop_notification_driver.moc"
