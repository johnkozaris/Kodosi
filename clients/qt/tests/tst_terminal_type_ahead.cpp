#include "presentation/TerminalTypeAhead.hpp"

#include <QKeyEvent>
#include <QTest>
#include <QWindow>

namespace {

class KeyWindow final : public QWindow {
public:
    QString typed;
    int released = 0;

protected:
    void keyPressEvent(QKeyEvent* event) override { typed += event->text(); }
    void keyReleaseEvent(QKeyEvent*) override { ++released; }
};

void type(QWindow* window, const QByteArray& text)
{
    for (const char key : text)
        QTest::keyClick(window, key);
}

}

class TerminalTypeAheadTests final : public QObject {
    Q_OBJECT

private slots:
    void theKeysWaitForTheNewTerminalAndKeepTheirOrder()
    {
        KeyWindow window;
        kodosi::TerminalTypeAhead typeAhead;
        typeAhead.hold();
        type(&window, "ls");
        QVERIFY(window.typed.isEmpty());
        typeAhead.expect(QStringLiteral("new"));
        typeAhead.deliver(QStringLiteral("old"));
        QVERIFY(typeAhead.holding());
        QVERIFY(window.typed.isEmpty());
        typeAhead.deliver(QStringLiteral("new"));
        QVERIFY(!typeAhead.holding());
        QTRY_COMPARE(window.typed, QStringLiteral("ls"));
        QCOMPARE(window.released, 2);
    }
    void keysAfterTheHoldGoStraightToTheWindow()
    {
        KeyWindow window;
        kodosi::TerminalTypeAhead typeAhead;
        typeAhead.hold();
        typeAhead.expect(QStringLiteral("new"));
        typeAhead.deliver(QStringLiteral("new"));
        type(&window, "ok");
        QCOMPARE(window.typed, QStringLiteral("ok"));
    }
    void aKeyThatWasDownBeforeTheHoldStillComesUp()
    {
        KeyWindow window;
        kodosi::TerminalTypeAhead typeAhead;
        QTest::keyPress(&window, Qt::Key_T);
        typeAhead.hold();
        QTest::keyRelease(&window, Qt::Key_T);
        QCOMPARE(window.released, 1);
        typeAhead.drop();
    }
    void droppedKeysReachNothing()
    {
        KeyWindow window;
        kodosi::TerminalTypeAhead typeAhead;
        typeAhead.hold();
        type(&window, "rm");
        typeAhead.drop();
        QTest::qWait(1);
        QVERIFY(window.typed.isEmpty());
    }
};

QTEST_MAIN(TerminalTypeAheadTests)
#include "tst_terminal_type_ahead.moc"
