#include "platform/DesktopFileIntegration.hpp"
#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <QtTest/QTest>
class DesktopFilesTest final : public QObject {
    Q_OBJECT
private slots:
    void opensOriginalFileWithoutChangingIt()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("provider-config.json"));
        QFile file(path);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("{\"untouched\":true}");
        file.close();
        QList<QUrl> opened;
        kodosi::DesktopFileIntegration files([&](const QUrl& url) {
            opened.append(url);
            return true;
        });
        QVERIFY(files.openPath(path));
        QCOMPARE(opened.last(), QUrl::fromLocalFile(path));
        QVERIFY(files.openPath(QDir::toNativeSeparators(path)));
        QCOMPARE(opened.last(), QUrl::fromLocalFile(path));
        QVERIFY(file.open(QIODevice::ReadOnly));
        QCOMPARE(file.readAll(), QByteArrayLiteral("{\"untouched\":true}"));
        QVERIFY(!files.openPath(QStringLiteral("https://example.com/config")));
        QVERIFY(!files.openPath(directory.filePath(QStringLiteral("missing"))));
        QVERIFY(!files.openWebUrl(QStringLiteral("file:///tmp/example")));
        QVERIFY(!files.openWebUrl(QStringLiteral("https://user:password@example.com")));
        QVERIFY(!files.openWebUrl(QStringLiteral("http://example.com/auth")));
        QVERIFY(files.openWebUrl(QStringLiteral("http://127.0.0.1:18080/realms/kodosi/auth")));
        QCOMPARE(opened.last(), QUrl(QStringLiteral("http://127.0.0.1:18080/realms/kodosi/auth")));
    }
    void pickerOpensOnceAndCancels()
    {
        kodosi::DesktopFileIntegration files;
        QVERIFY(files.requestDirectory(QStringLiteral("new")));
        QVERIFY(files.busy());
        QVERIFY(!files.requestDirectory(QStringLiteral("resume")));
        files.cancelDirectory();
        QTRY_VERIFY(!files.busy());
    }
};
QTEST_MAIN(DesktopFilesTest)
#include "tst_desktop_file_integration.moc"
