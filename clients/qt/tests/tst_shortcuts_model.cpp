#include "presentation/ShortcutsModel.hpp"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest/QTest>

#include <memory>

using kodosi::ShortcutsModel;

namespace {

std::unique_ptr<QSettings> settingsIn(const QTemporaryDir& dir)
{
    return std::make_unique<QSettings>(dir.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);
}

QStringList keysOf(const ShortcutsModel& model, const QString& action)
{
    return model.keys().value(action).toStringList();
}

}

class ShortcutsModelTest final : public QObject {
    Q_OBJECT

private slots:
    void eachDefaultKeyDoesOneThing()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        const auto taken = model.taken();
        for (const auto& key : taken)
            QCOMPARE(taken.count(key), 1);
        QVERIFY(!model.changed());
    }

    void aProgramKeepsItsKeysUnlessTheTableTakesThem()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        const auto taken = model.taken();
        QVERIFY(taken.contains(QKeySequence(QStringLiteral("Alt+7"))));
        QVERIFY(taken.contains(QKeySequence(QStringLiteral("Ctrl+Tab"))));
        QVERIFY(!taken.contains(QKeySequence(QStringLiteral("Ctrl+K"))));
        QVERIFY(!taken.contains(QKeySequence(QStringLiteral("Ctrl+C"))));
    }

    void aKeyMovesToTheActionThatTakesIt()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        QSignalSpy changed(&model, &ShortcutsModel::keysChanged);

        const auto holder = model.bind(
            QStringLiteral("goTo"), QStringLiteral("Ctrl+Shift+T"), QStringLiteral("Ctrl+Shift+K"));

        QCOMPARE(holder, QStringLiteral("Quick terminal"));
        QCOMPARE(keysOf(model, QStringLiteral("goTo")), QStringList(QStringLiteral("Ctrl+Shift+T")));
        QVERIFY(!keysOf(model, QStringLiteral("quickTerminal")).contains(QStringLiteral("Ctrl+Shift+T")));
        QCOMPARE(changed.count(), 1);
        QVERIFY(model.changed());
    }

    void changesStayAndAResetEndsThem()
    {
        QTemporaryDir dir;
        {
            ShortcutsModel model(settingsIn(dir));
            model.remove(QStringLiteral("close"), QStringLiteral("Ctrl+Shift+W"));
            model.bind(QStringLiteral("numberedTerminal"), QStringLiteral("Ctrl+Alt+4"), QStringLiteral("Alt+1"));
        }
        ShortcutsModel model(settingsIn(dir));
        QVERIFY(keysOf(model, QStringLiteral("close")).isEmpty());
        QCOMPARE(keysOf(model, QStringLiteral("numberedTerminal")), QStringList(QStringLiteral("Ctrl+Alt+1")));
        QVERIFY(model.taken().contains(QKeySequence(QStringLiteral("Ctrl+Alt+9"))));

        model.reset();
        QVERIFY(!model.changed());
        QCOMPARE(keysOf(model, QStringLiteral("close")), QStringList(QStringLiteral("Ctrl+Shift+W")));
    }

    void bracketsListTheKeyThatTheLayoutSends()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        QVERIFY(keysOf(model, QStringLiteral("previousTerminal")).contains(QStringLiteral("Ctrl+{")));
        QVERIFY(keysOf(model, QStringLiteral("nextTerminal")).contains(QStringLiteral("Ctrl+}")));
    }

    void plainKeysStayWithTheTerminal()
    {
        QCOMPARE(ShortcutsModel::sequence(Qt::Key_A, Qt::NoModifier), QString());
        QCOMPARE(ShortcutsModel::sequence(Qt::Key_A, Qt::ShiftModifier), QString());
        QCOMPARE(ShortcutsModel::sequence(Qt::Key_Control, Qt::ControlModifier), QString());
        QCOMPARE(ShortcutsModel::sequence(Qt::Key_F5, Qt::NoModifier), QStringLiteral("F5"));
        QCOMPARE(ShortcutsModel::sequence(Qt::Key_T, Qt::ControlModifier | Qt::ShiftModifier), QStringLiteral("Ctrl+Shift+T"));
    }
};

QTEST_GUILESS_MAIN(ShortcutsModelTest)
#include "tst_shortcuts_model.moc"
