#include "app/DeepLinkRouter.hpp"
#include <QtTest/QTest>

class DeepLinksTest final : public QObject {
    Q_OBJECT
private slots:
    void acceptsSession()
    {
        const auto parsed = kodosi::DeepLinkRouter::parse(
            QStringLiteral("kodosi://session/0198AAAA-0000-7000-8000-00000000000B"));
        QVERIFY(parsed.destination.has_value());
        QCOMPARE(parsed.destination->sessionId, QStringLiteral("0198aaaa-0000-7000-8000-00000000000b"));
        QCOMPARE(kodosi::DeepLinkRouter::parse(kodosi::DeepLinkRouter::url(*parsed.destination)).destination,
            parsed.destination);
    }
    void rejectsOtherDestinations_data()
    {
        QTest::addColumn<QString>("url");
        for (const auto& value :
            { "https://session/123", "kodosi://session/123?toolUseId=abc", "kodosi://session/123?x=y",
                "kodosi://session/123#x", "kodosi://session/123/456", "kodosi://session/",
                "kodosi://session/%00", "kodosi://session/%2f", "kodosi://session/%GG", "kodosi://session/123",
                "kodosi://session/{0198aaaa-0000-7000-8000-000000000001}",
                "kodosi://session/0198aaaa%2D0000-7000-8000-000000000001",
                "kodosi://session/00000000-0000-0000-0000-000000000000" })
            QTest::newRow(value) << QString::fromUtf8(value);
    }
    void rejectsOtherDestinations()
    {
        QFETCH(QString, url);
        QVERIFY(!kodosi::DeepLinkRouter::parse(url).destination.has_value());
    }
};
QTEST_GUILESS_MAIN(DeepLinksTest)
#include "tst_deep_links.moc"
