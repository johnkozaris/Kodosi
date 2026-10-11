#include "app/DeepLinkRouter.hpp"
#include <QtTest/QTest>

class DeepLinksTest final : public QObject {
    Q_OBJECT
private slots:
    void acceptsSession()
    {
        const auto parsed = kodosi::DeepLinkRouter::parse(
            QStringLiteral("kodosi://session/0198aaaa-0000-7000-8000-000000000001"));
        QVERIFY(parsed.destination.has_value());
        QCOMPARE(parsed.destination->sessionId, QStringLiteral("0198aaaa-0000-7000-8000-000000000001"));
    }
    void rejectsOtherDestinations_data()
    {
        QTest::addColumn<QString>("url");
        for (const auto& value :
            { "https://session/123", "kodosi://session/123?toolUseId=abc", "kodosi://session/123?x=y",
                "kodosi://session/123#x", "kodosi://session/123/456", "kodosi://session/",
                "kodosi://session/%00", "kodosi://session/%2f", "kodosi://session/%GG" })
            QTest::newRow(value) << QString::fromUtf8(value);
    }
    void rejectsOtherDestinations()
    {
        QFETCH(QString, url);
        QVERIFY(!kodosi::DeepLinkRouter::parse(url).destination.has_value());
    }
    void urlRoundTrips_data()
    {
        QTest::addColumn<QString>("sessionId");
        QTest::newRow("uuid") << QStringLiteral("0198aaaa-0000-7000-8000-000000000001");
        QTest::newRow("reserved") << QStringLiteral("a b%c&d=e+f:g");
        QTest::newRow("unicode") << QStringLiteral("caf\u00e9-\U0001F680");
    }
    void urlRoundTrips()
    {
        QFETCH(QString, sessionId);
        const auto url = kodosi::DeepLinkRouter::url({ .sessionId = sessionId });
        QVERIFY(url.startsWith(QStringLiteral("kodosi://session/")));
        const auto parsed = kodosi::DeepLinkRouter::parse(url);
        QVERIFY(parsed.destination.has_value());
        QCOMPARE(parsed.destination->sessionId, sessionId);
    }
};
QTEST_GUILESS_MAIN(DeepLinksTest)
#include "tst_deep_links.moc"
