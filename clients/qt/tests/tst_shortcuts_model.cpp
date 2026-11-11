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
        QVERIFY(!taken.contains(QKeySequence(QStringLiteral("F11"))));
    }

    void aKeyMovesToTheActionThatTakesIt()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        QSignalSpy changed(&model, &ShortcutsModel::keysChanged);

        const auto holder = model.bind(
            QStringLiteral("goTo"), QStringLiteral("Ctrl+Shift+T"), QStringLiteral("Ctrl+Shift+K"));

        QCOMPARE(holder, QStringLiteral("Same agent"));
        QCOMPARE(keysOf(model, QStringLiteral("goTo")), QStringList(QStringLiteral("Ctrl+Shift+T")));
        QVERIFY(keysOf(model, QStringLiteral("sameAgent")).isEmpty());
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
        model.bind(QStringLiteral("numberedTerminal"), QStringLiteral("Ctrl+Alt+T"));
        QCOMPARE(keysOf(model, QStringLiteral("numberedTerminal")), QStringList(QStringLiteral("Ctrl+Alt+1")));
        QVERIFY(model.taken().contains(QKeySequence(QStringLiteral("Ctrl+Alt+9"))));

        model.reset();
        QVERIFY(!model.changed());
        QCOMPARE(keysOf(model, QStringLiteral("close")), QStringList(QStringLiteral("Ctrl+Shift+W")));
    }

    void eachKeyOfAnActionIsOneShortcut()
    {
        QTemporaryDir dir;
        ShortcutsModel model(settingsIn(dir));
        QCOMPARE(keysOf(model, QStringLiteral("previousTerminal")),
            (QStringList { QStringLiteral("Ctrl+Shift+["), QStringLiteral("Ctrl+Shift+Left"), QStringLiteral("Ctrl+PgUp"),
                QStringLiteral("Ctrl+Shift+Tab") }));
        QCOMPARE(keysOf(model, QStringLiteral("nextTerminal")),
            (QStringList { QStringLiteral("Ctrl+Shift+]"), QStringLiteral("Ctrl+Shift+Right"), QStringLiteral("Ctrl+PgDown"),
                QStringLiteral("Ctrl+Tab") }));
    }

    void aPressedKeyIsRecordedAsTheShortcutsMatchIt()
    {
        const auto held = Qt::ControlModifier | Qt::ShiftModifier;
        QCOMPARE(ShortcutsModel::sequence(held | Qt::Key_BraceLeft, { held | Qt::Key_BracketLeft, Qt::ControlModifier | Qt::Key_BraceLeft }),
            QStringLiteral("Ctrl+Shift+["));
        QCOMPARE(ShortcutsModel::sequence(held | Qt::Key_Backtab, { held | Qt::Key_Tab }), QStringLiteral("Ctrl+Shift+Tab"));
        QCOMPARE(ShortcutsModel::sequence(held | Qt::Key_Exclam, { held | Qt::Key_1, Qt::ControlModifier | Qt::Key_Exclam }),
            QStringLiteral("Ctrl+Shift+1"));
        QCOMPARE(ShortcutsModel::sequence(held | Qt::Key_T, {}), QStringLiteral("Ctrl+Shift+T"));
    }

    void plainKeysStayWithTheTerminal()
    {
        QCOMPARE(ShortcutsModel::sequence(QKeyCombination(Qt::Key_A), { Qt::Key_A }), QString());
        QCOMPARE(ShortcutsModel::sequence(Qt::ShiftModifier | Qt::Key_A, { Qt::ShiftModifier | Qt::Key_A }), QString());
        QCOMPARE(ShortcutsModel::sequence(Qt::ControlModifier | Qt::Key_Control, {}), QString());
        QCOMPARE(ShortcutsModel::sequence(QKeyCombination(Qt::Key_F5), { Qt::Key_F5 }), QStringLiteral("F5"));
    }
};

QTEST_GUILESS_MAIN(ShortcutsModelTest)
#include "tst_shortcuts_model.moc"
