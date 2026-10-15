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
    void withoutAChosenFolderTheLauncherStartsInTheHomeFolder()
    {
        QTemporaryDir directory;
        kodosi::DesktopSettings settings(
            std::make_unique<QSettings>(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat));
        QCOMPARE(settings.homeDirectory(), QDir::homePath());
        QCOMPARE(settings.effectiveWorkingDirectory(), settings.homeDirectory());
    }
    void oneRuleNamesTheAgentOfACommandOrAName()
    {
        using kodosi::DesktopSettings;
        QCOMPARE(DesktopSettings::agentOf(QStringLiteral("FOO=1 /usr/local/bin/codex --full-auto")), QStringLiteral("codex"));
        QCOMPARE(DesktopSettings::agentOf(QStringLiteral("env CLAUDE_CONFIG_DIR=~/.claude_work codex")), QStringLiteral("codex"));
        QCOMPARE(DesktopSettings::agentOf(QStringLiteral("cursor-agent")), QStringLiteral("cursor"));
        QCOMPARE(DesktopSettings::agentOf(QStringLiteral("GitHub Copilot")), QStringLiteral("copilot"));
        QCOMPARE(DesktopSettings::agentOf(QStringLiteral("npm run dev")), QStringLiteral("shell"));
        QCOMPARE(DesktopSettings::agentLabel(QStringLiteral("Claude")), QStringLiteral("Claude Code"));
        QCOMPARE(DesktopSettings::agentLabel({}), QStringLiteral("Shell"));
    }
};
QTEST_GUILESS_MAIN(DesktopSettingsTest)
#include "tst_desktop_settings.moc"
