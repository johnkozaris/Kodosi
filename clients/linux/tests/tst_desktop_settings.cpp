#include "presentation/DesktopSettings.hpp"
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
};
QTEST_GUILESS_MAIN(DesktopSettingsTest)
#include "tst_desktop_settings.moc"
