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
    void anAgentThatRunsHereGetsOneStartCommand()
    {
        QTemporaryDir directory;
        const auto path = directory.filePath(QStringLiteral("settings.ini"));
        const auto names = [](const kodosi::DesktopSettings& settings) {
            QStringList result;
            for (const auto& entry : settings.startCommands())
                result.append(entry.toMap().value(QStringLiteral("name")).toString());
            return result;
        };
        {
            kodosi::DesktopSettings settings(std::make_unique<QSettings>(path, QSettings::IniFormat));
            QVERIFY(settings.startCommands().isEmpty());
            settings.learn(QStringLiteral("vim"));
            settings.learn(QStringLiteral("claude"));
            settings.learn(QStringLiteral("claude"));
            QCOMPARE(names(settings), QStringList { QStringLiteral("Claude Code") });
            settings.addStartCommand();
            const auto added = settings.startCommandIds().constLast();
            settings.setStartCommand(added, QStringLiteral("Claude personal"),
                QStringLiteral("env CLAUDE_CONFIG_DIR=~/.claude_personal claude"));
            const auto second = settings.startCommands().constLast().toMap();
            QCOMPARE(second.value(QStringLiteral("agent")).toString(), QStringLiteral("claude"));
            QVERIFY(second.value(QStringLiteral("repeated")).toBool());
            QCOMPARE(second.value(QStringLiteral("initial")).toString(), QStringLiteral("P"));
            settings.removeStartCommand(settings.startCommandIds().constFirst());
        }
        kodosi::DesktopSettings restored(std::make_unique<QSettings>(path, QSettings::IniFormat));
        restored.learn(QStringLiteral("claude"));
        restored.learn(QStringLiteral("cursor"));
        QCOMPARE(names(restored), (QStringList { QStringLiteral("Claude personal"), QStringLiteral("Cursor") }));
        QCOMPARE(restored.startCommands().constLast().toMap().value(QStringLiteral("command")).toString(),
            QStringLiteral("cursor-agent"));
        QCOMPARE(kodosi::DesktopSettings::agentOf(QStringLiteral("FOO=1 /usr/local/bin/codex --full-auto")), QStringLiteral("codex"));
        QCOMPARE(kodosi::DesktopSettings::agentOf(QStringLiteral("npm run dev")), QStringLiteral("shell"));
    }
};
QTEST_GUILESS_MAIN(DesktopSettingsTest)
#include "tst_desktop_settings.moc"
