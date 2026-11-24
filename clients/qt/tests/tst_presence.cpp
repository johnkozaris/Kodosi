#include "platform/Presence.hpp"
#include "runtime/RuntimeBridge.hpp"

#include <QJsonDocument>
#include <QKeyEvent>
#include <QtTest/QTest>

using namespace std::chrono_literals;
using kodosi::Presence;
using Input = kodosi::PresenceSource::Input;

Q_DECLARE_METATYPE(kodosi::PresenceFacts)
Q_DECLARE_METATYPE(kodosi::PresenceDecision)

namespace {

class Commands final : public kodosi::CommandDispatcher {
public:
    QList<QJsonObject> sent;
    Result send(QByteArrayView bytes) override
    {
        sent.append(QJsonDocument::fromJson(bytes.toByteArray()).object());
        return {};
    }
    [[nodiscard]] QStringList states() const
    {
        QStringList states;
        for (const auto& command : sent)
            states.append(command.value(QStringLiteral("state")).toString());
        return states;
    }
};

class Source final : public kodosi::PresenceSource {
public:
    explicit Source(bool reports)
        : m_reports(reports)
    {
    }
    Input given {};
    bool locked = false;
    std::optional<std::chrono::milliseconds> awayTime;

    bool reportsInput() const override { return m_reports; }
    Input input() override { return given; }
    bool away() const override { return locked; }
    void setAwayTime(std::chrono::milliseconds time) override { awayTime = time; }
    void tell() { emit changed(); }

private:
    bool m_reports;
};

QJsonObject ready(int awaySeconds)
{
    QJsonObject event { { QStringLiteral("type"), QStringLiteral("system.ready") } };
    if (awaySeconds > 0)
        event.insert(QStringLiteral("awaySeconds"), awaySeconds);
    return event;
}

void press()
{
    QObject target;
    QKeyEvent key(QEvent::KeyPress, Qt::Key_A, Qt::NoModifier);
    QCoreApplication::sendEvent(&target, &key);
}

}

class PresenceTest final : public QObject {
    Q_OBJECT

private slots:
    void decidesFromTheFacts_data();
    void decidesFromTheFacts();
    void reportsAfterEachReadyAndThenOnlyChanges();
    void readsTheSystemAgainWhenItSaysSo();
    void inputIntoKodosiDecidesAtOnce();
    void withoutSystemInputOnlyInputIntoKodosiCounts();
};

void PresenceTest::decidesFromTheFacts_data()
{
    QTest::addColumn<kodosi::PresenceFacts>("facts");
    QTest::addColumn<kodosi::PresenceDecision>("decision");
    const Input recent { .quiet = false, .readAgainIn = 40s };
    const Input quiet { .quiet = true, .readAgainIn = std::nullopt };
    QTest::newRow("locked")
        << kodosi::PresenceFacts { .active = true, .away = true, .system = recent, .kodosiInputAge = 0s }
        << kodosi::PresenceDecision { .state = Presence::Away, .next = std::nullopt };
    QTest::newRow("in Kodosi") << kodosi::PresenceFacts { .active = true, .away = false, .system = recent, .kodosiInputAge = 0s }
                               << kodosi::PresenceDecision { .state = Presence::Kodosi, .next = 40s };
    QTest::newRow("in another app")
        << kodosi::PresenceFacts { .active = false, .away = false, .system = recent, .kodosiInputAge = 900s }
        << kodosi::PresenceDecision { .state = Presence::Here, .next = 40s };
    QTest::newRow("no input for the away time")
        << kodosi::PresenceFacts { .active = true, .away = false, .system = quiet, .kodosiInputAge = 0s }
        << kodosi::PresenceDecision { .state = Presence::Idle, .next = std::nullopt };
    QTest::newRow("only Kodosi's input, recent")
        << kodosi::PresenceFacts { .active = true, .away = false, .system = std::nullopt, .kodosiInputAge = 60s }
        << kodosi::PresenceDecision { .state = Presence::Kodosi, .next = 120s };
    QTest::newRow("only Kodosi's input, quiet")
        << kodosi::PresenceFacts { .active = true, .away = false, .system = std::nullopt, .kodosiInputAge = 180s }
        << kodosi::PresenceDecision { .state = Presence::Idle, .next = std::nullopt };
    QTest::newRow("only Kodosi's input, another app in front")
        << kodosi::PresenceFacts { .active = false, .away = false, .system = std::nullopt, .kodosiInputAge = 1s }
        << kodosi::PresenceDecision { .state = Presence::Idle, .next = std::nullopt };
}

void PresenceTest::decidesFromTheFacts()
{
    QFETCH(kodosi::PresenceFacts, facts);
    QFETCH(kodosi::PresenceDecision, decision);
    QVERIFY(kodosi::decidePresence(facts, 180s) == decision);
}

void PresenceTest::reportsAfterEachReadyAndThenOnlyChanges()
{
    Commands commands;
    Source source(true);
    source.given = { .quiet = false, .readAgainIn = std::nullopt };
    kodosi::PresenceReporter reporter(commands, source);
    source.tell();
    reporter.setActive(true);
    QVERIFY(commands.sent.isEmpty());

    reporter.apply(ready(180));
    QVERIFY(source.awayTime == std::chrono::milliseconds(180s));
    QCOMPARE(commands.states(), QStringList { QStringLiteral("kodosi") });

    source.tell();
    reporter.setActive(true);
    QCOMPARE(commands.sent.size(), 1);
    reporter.setActive(false);
    source.given.quiet = true;
    source.tell();
    source.locked = true;
    source.tell();
    QCOMPARE(commands.states(),
        QStringList({ QStringLiteral("kodosi"), QStringLiteral("here"), QStringLiteral("idle"), QStringLiteral("away") }));

    reporter.apply(ready(180));
    QCOMPARE(commands.states().constLast(), QStringLiteral("away"));
    QCOMPARE(commands.sent.size(), 5);

    reporter.apply(ready(0));
    source.locked = false;
    source.tell();
    QCOMPARE(commands.sent.size(), 5);
}

void PresenceTest::readsTheSystemAgainWhenItSaysSo()
{
    Commands commands;
    Source source(true);
    source.given = { .quiet = false, .readAgainIn = 30ms };
    kodosi::PresenceReporter reporter(commands, source);
    reporter.apply(ready(180));
    QCOMPARE(commands.states(), QStringList { QStringLiteral("here") });
    source.given = { .quiet = true, .readAgainIn = 30ms };
    QTRY_COMPARE(commands.states(), QStringList({ QStringLiteral("here"), QStringLiteral("idle") }));
    source.given = { .quiet = false, .readAgainIn = 1h };
    QTRY_COMPARE(commands.states(), QStringList({ QStringLiteral("here"), QStringLiteral("idle"), QStringLiteral("here") }));
}

void PresenceTest::inputIntoKodosiDecidesAtOnce()
{
    Commands commands;
    Source source(true);
    source.given = { .quiet = true, .readAgainIn = 1h };
    kodosi::PresenceReporter reporter(commands, source);
    reporter.apply(ready(180));
    QCOMPARE(commands.states(), QStringList { QStringLiteral("idle") });
    source.given = { .quiet = false, .readAgainIn = 1h };
    press();
    QCOMPARE(commands.states(), QStringList({ QStringLiteral("idle"), QStringLiteral("here") }));
}

void PresenceTest::withoutSystemInputOnlyInputIntoKodosiCounts()
{
    Commands commands;
    Source source(false);
    kodosi::PresenceReporter reporter(commands, source);
    reporter.apply(ready(1));
    QCOMPARE(commands.states(), QStringList { QStringLiteral("idle") });

    reporter.setActive(true);
    QCOMPARE(commands.states().constLast(), QStringLiteral("kodosi"));
    QTRY_COMPARE_WITH_TIMEOUT(commands.states().constLast(), QStringLiteral("idle"), 3000);

    press();
    QCOMPARE(commands.states(),
        QStringList({ QStringLiteral("idle"), QStringLiteral("kodosi"), QStringLiteral("idle"), QStringLiteral("kodosi") }));
    reporter.setActive(false);
    QCOMPARE(commands.states().constLast(), QStringLiteral("idle"));
    QVERIFY(!commands.states().contains(QStringLiteral("here")));
}

QTEST_MAIN(PresenceTest)

#include "tst_presence.moc"
