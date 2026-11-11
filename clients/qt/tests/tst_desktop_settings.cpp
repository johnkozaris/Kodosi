#include "presentation/DesktopSettings.hpp"
#include <QDir>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QtTest/QTest>
class DesktopSettingsTest final : public QObject {
    Q_OBJECT
private slots:
    void validatesAndPersists()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("settings.ini"));
        kodosi::DesktopSettings settings(std::make_unique<QSettings>(path, QSettings::IniFormat));
        QVERIFY(!settings.apply(QStringLiteral("monospace"), 100, 0, 1.0, 1000, true));
        QVERIFY(settings.apply(QStringLiteral("monospace"), 16, 1, 1.2, 5000, true));
        kodosi::DesktopSettings restored(std::make_unique<QSettings>(path, QSettings::IniFormat));
        QCOMPARE(restored.fontSize(), 16);
        QCOMPARE(restored.cursorStyle(), kodosi::DesktopSettings::Bar);
        QCOMPARE(restored.scrollbackLines(), 5000);
        QVERIFY(restored.cursorBlink());
        restored.setWorkingDirectory(directory.path());
        QCOMPARE(restored.effectiveWorkingDirectory(), directory.path());
        restored.resetTerminal();
        QCOMPARE(restored.fontSize(), 13);
        QVERIFY(!restored.cursorBlink());
    }
    void theBranchFolderStaysAfterARestart()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("settings.ini"));
        {
            kodosi::DesktopSettings settings(std::make_unique<QSettings>(path, QSettings::IniFormat));
            QVERIFY(settings.branchFolder().isEmpty());
            settings.setBranchFolder(QStringLiteral("relative"));
            settings.setBranchFolder(directory.filePath(QStringLiteral("missing")));
            QVERIFY(settings.branchFolder().isEmpty());
            settings.setBranchFolder(directory.path());
            QCOMPARE(settings.branchFolder(), QFileInfo(directory.path()).canonicalFilePath());
        }
        {
            kodosi::DesktopSettings restored(std::make_unique<QSettings>(path, QSettings::IniFormat));
            QCOMPARE(restored.branchFolder(), QFileInfo(directory.path()).canonicalFilePath());
            restored.setBranchFolder({});
        }
        QVERIFY(kodosi::DesktopSettings(std::make_unique<QSettings>(path, QSettings::IniFormat)).branchFolder().isEmpty());
    }
    void aViewerThatIsOffLeavesItsFilesAndItsChoicesStay()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("settings.ini"));
        {
            kodosi::DesktopSettings settings(std::make_unique<QSettings>(path, QSettings::IniFormat));
            QCOMPARE(settings.shownViews().value(QStringLiteral("xlsx")).toBool(), true);
            QCOMPARE(settings.viewPreferences().value(QStringLiteral("xlsx")).toMap().value(QStringLiteral("dark")).toBool(), true);
            settings.setViewer(QStringLiteral("sheets"), false);
            settings.setViewer(QStringLiteral("documentPages"), false);
            settings.setViewer(QStringLiteral("unknown"), false);
        }
        kodosi::DesktopSettings restored(std::make_unique<QSettings>(path, QSettings::IniFormat));
        QCOMPARE(restored.shownViews().value(QStringLiteral("xlsx")).toBool(), false);
        QCOMPARE(restored.shownViews().value(QStringLiteral("docx")).toBool(), true);
        const auto document = restored.viewPreferences().value(QStringLiteral("docx")).toMap();
        QCOMPARE(document.value(QStringLiteral("pages")).toBool(), false);
        QCOMPARE(document.value(QStringLiteral("dark")).toBool(), false);
        QVERIFY(!restored.viewers().contains(QStringLiteral("unknown")));
    }
    void withoutASystemTrayClosingTheWindowQuitsUntilThePersonKeepsKodosiRunning()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("settings.ini"));
        const auto settings = [&path](bool tray) {
            return std::make_unique<kodosi::DesktopSettings>(std::make_unique<QSettings>(path, QSettings::IniFormat), tray);
        };
        {
            const auto withoutTray = settings(false);
            QVERIFY(!withoutTray->keepRunning());
            withoutTray->setLastComputer(QStringLiteral("studio"));
        }
        QVERIFY(settings(true)->keepRunning());
        settings(false)->setKeepRunning(true);
        QVERIFY(settings(false)->keepRunning());
    }
};
QTEST_GUILESS_MAIN(DesktopSettingsTest)
#include "tst_desktop_settings.moc"
