#include "platform/linux/FreedesktopNotificationDriver.hpp"

#include <QDBusConnection>
#include <QDBusPendingCallWatcher>
#include <QSignalSpy>
#include <QtTest/QTest>

class FakeFreedesktopNotificationService final : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.Notifications")

public slots:
    uint Notify(const QString&, uint, const QString&, const QString&, const QString&, const QStringList&,
        const QVariantMap&, int)
    {
        return 42;
    }

    void CloseNotification(const uint id)
    {
        emit NotificationClosed(id, 3);
    }

signals:
    void NotificationClosed(uint id, uint reason);
};

class FreedesktopNotificationDriverTest final : public QObject {
    Q_OBJECT

private slots:
    void initTestCase();
    void cleanupTestCase();
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

void FreedesktopNotificationDriverTest::pendingPostsRemainBoundedAfterWithdrawal()
{
    kodosi::FreedesktopNotificationDriver driver;
    QSignalSpy closed(&driver, &kodosi::DesktopNotificationDriver::notificationClosed);
    for (int i = 0; i < 1000; ++i) {
        const auto key = QString::number(i);
        driver.post({ .key = key, .title = QStringLiteral("Terminal"), .body = QStringLiteral("Complete"),
            .sessionId = QStringLiteral("0198aaaa-0000-7000-8000-000000000001"), .computer = {},
            .cue = kodosi::NotificationCue::List });
        driver.withdraw(key);
    }
    QCOMPARE(closed.count(), 968);
    QVERIFY(driver.findChildren<QDBusPendingCallWatcher*>().size() <= 32);
    QTRY_VERIFY(driver.findChildren<QDBusPendingCallWatcher*>().isEmpty());
}

QTEST_GUILESS_MAIN(FreedesktopNotificationDriverTest)

#include "tst_freedesktop_notification_driver.moc"
