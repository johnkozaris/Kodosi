#include "platform/DesktopFileIntegration.hpp"
#include <QFile>
#include <QSignalSpy>
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
        QVERIFY(file.open(QIODevice::ReadOnly));
        QCOMPARE(file.readAll(), QByteArrayLiteral("{\"untouched\":true}"));
        QVERIFY(!files.openPath(QStringLiteral("https://example.com/config")));
        QVERIFY(!files.openPath(directory.filePath(QStringLiteral("missing"))));
        QVERIFY(!files.openWebUrl(QStringLiteral("file:///tmp/example")));
        QVERIFY(!files.openWebUrl(QStringLiteral("https://user:password@example.com")));
    }
    void pickerCancellationIsCorrelated()
    {
        kodosi::DesktopFileIntegration files;
        QSignalSpy cancelled(&files, &kodosi::DesktopFileIntegration::directoryPickCancelled);
        QVERIFY(files.requestDirectory(QStringLiteral("new")));
        QVERIFY(files.busy());
        QVERIFY(!files.requestDirectory(QStringLiteral("resume")));
        files.cancelDirectory();
        QTRY_COMPARE(cancelled.size(), 1);
        QVERIFY(!files.busy());
        QCOMPARE(cancelled.first().first().toString(), QStringLiteral("new"));
    }
};
QTEST_MAIN(DesktopFilesTest)
#include "tst_desktop_file_integration.moc"
