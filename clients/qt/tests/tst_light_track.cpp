#include "app/QmlModelTypes.hpp"
#include "controls/LightTrack.hpp"

#include <QColor>
#include <QDateTime>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QSettings>
#include <QTemporaryDir>
#include <QtQml/QQmlExtensionPlugin>
#include <QtTest/QTest>

#include <algorithm>
#include <cmath>
#include <memory>
#include <numbers>

Q_IMPORT_QML_PLUGIN(KodosiPlugin)

namespace {

using kodosi::light::Mesh;

const QColor tileLight(0xea, 0xa4, 0x7e);

double angleOf(QPointF centre, const QSGGeometry::ColoredPoint2D& vertex)
{
    const double degrees = std::atan2(vertex.x - centre.x(), centre.y() - vertex.y) * 180 / std::numbers::pi;
    return degrees < 0 ? degrees + 360 : degrees;
}

double apart(double first, double second)
{
    const double gap = std::fmod(std::abs(first - second), 360.0);
    return std::min(gap, 360 - gap);
}

double distance(QPointF point, const QSGGeometry::ColoredPoint2D& vertex)
{
    return std::hypot(vertex.x - point.x(), vertex.y - point.y());
}

int alphaAt(const Mesh& mesh, double degrees)
{
    const auto& vertices = mesh.vertices();
    for (std::size_t index = 1; index < vertices.size(); index += 4) {
        if (apart(angleOf(mesh.centre(), vertices[index]), degrees) < 0.01)
            return vertices[index].a;
    }
    return -1;
}

}

class LightTrackTest final : public QObject {
    Q_OBJECT
private slots:
    void theLightLeadsWithAShortEdgeAndTrailsALongerOne()
    {
        QCOMPARE(kodosi::light::glow(0), 1.0);
        QVERIFY(std::abs(kodosi::light::glow(20) - std::exp(-0.5)) < 1e-9);
        QVERIFY(std::abs(kodosi::light::glow(-36) - std::exp(-0.5)) < 1e-9);
        QVERIFY(kodosi::light::glow(180) < 1e-3);
        QVERIFY(kodosi::light::glow(-180) < 1e-3);

        Mesh ring({ QSizeF(38, 38), -1, 2, 1, false });
        const int flank = qRound((0.2 + 0.8 * std::exp(-0.5)) * 255);
        ring.paint({ .angle = 99, .light = QColor(Qt::red) });
        QCOMPARE(alphaAt(ring, 99), 255);
        QCOMPARE(alphaAt(ring, 63), flank);
        QCOMPARE(alphaAt(ring, 279), qRound(0.2 * 255));
        ring.paint({ .angle = 100, .light = QColor(Qt::red) });
        QCOMPARE(alphaAt(ring, 120), flank);
    }

    void everyLightTurnsWithTheWallClockLikeTheMac()
    {
        QCOMPARE(kodosi::light::phase(0), 0.0);
        QCOMPARE(kodosi::light::phase(1000), 90.0);
        QCOMPARE(kodosi::light::phase(3500), 315.0);
        QCOMPARE(kodosi::light::phase(4000), 0.0);
        QCOMPARE(kodosi::light::phase(-1000), 270.0);
        const std::int64_t macReferenceDate = 978'307'200'000;
        for (const std::int64_t since : { 0LL, 1'250LL, 777'777'777LL, 1'000'000'000'123LL }) {
            const double turn = 360 * std::fmod(static_cast<double>(since) / 1000, 4) / 4;
            QVERIFY(apart(kodosi::light::phase(macReferenceDate + since), turn) < 1e-3);
        }
    }

    void theLightFollowsTheEdgeOfTheTile()
    {
        Mesh tile({ QSizeF(44, 44), 13.2, 3.08, 1, false });
        tile.paint({ .angle = 0, .light = tileLight });
        const auto& vertices = tile.vertices();
        QCOMPARE(vertices.size(), std::size_t { 4 * kodosi::light::rays });
        const std::array top { -0.5, 0.5, 2.58, 3.58 };
        for (std::size_t across = 0; across < top.size(); ++across) {
            QVERIFY(std::abs(vertices[across].x - 22) < 1e-3);
            QVERIFY(std::abs(vertices[across].y - top[across]) < 1e-3);
        }
        const double radius = 13.2 - 3.08 / 2;
        const QPointF corner(44 - 3.08 / 2 - 0.291506715 * radius, 3.08 / 2 + 0.291506715 * radius);
        const auto& outer = vertices[4 * 15 + 1];
        const auto& inner = vertices[4 * 15 + 2];
        QVERIFY(std::hypot((outer.x + inner.x) / 2 - corner.x(), (outer.y + inner.y) / 2 - corner.y()) < 1e-3);

        Mesh ring({ QSizeF(38, 38), -1, 2, 1, false });
        ring.paint({ .angle = 0, .light = QColor(Qt::red) });
        const std::array radii { 19.5, 18.5, 17.5, 16.5 };
        for (std::size_t index = 0; index < ring.vertices().size(); ++index)
            QVERIFY(std::abs(distance(ring.centre(), ring.vertices()[index]) - radii[index % 4]) < 1e-3);
    }

    void aPercentLightsTheEdgeClockwiseFromTheTop()
    {
        Mesh tile({ QSizeF(44, 44), 13.2, 3.08, 1, true });
        const int track = qRound(0.22 * 255);
        for (const double fraction : { 0.25, 0.6 }) {
            tile.paint({ .progress = true, .fraction = fraction, .light = tileLight });
            const auto& vertices = tile.vertices();
            QCOMPARE(vertices.size(), std::size_t { 4 * (kodosi::light::rays + 5) });
            int lit = 0;
            int unlit = 0;
            for (std::size_t index = 0; index < vertices.size(); ++index) {
                if (index % 4 == 0 || index % 4 == 3) {
                    QCOMPARE(int { vertices[index].a }, 0);
                    continue;
                }
                const double angle = angleOf(tile.centre(), vertices[index]);
                if (angle > 3 && angle < 360 * fraction - 3) {
                    QCOMPARE(int { vertices[index].a }, 255);
                    ++lit;
                } else if (angle > 360 * fraction + 3 && angle < 357) {
                    QCOMPARE(int { vertices[index].a }, track);
                    ++unlit;
                }
            }
            QVERIFY(lit > 0 && unlit > 0);
        }
        for (const auto& [fraction, alpha] : { std::pair { 1.0, 255 }, std::pair { 0.0, track } }) {
            tile.paint({ .progress = true, .fraction = fraction, .light = tileLight });
            for (std::size_t index = 1; index < tile.vertices().size(); index += 4)
                QCOMPARE(int { tile.vertices()[index].a }, alpha);
        }
    }

    void aStillOrHiddenLightRestsAtTheTopLeft()
    {
        QQuickWindow window;
        window.resize(120, 80);
        kodosi::LightTrack light(window.contentItem());
        light.setSize(QSizeF(44, 44));
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));
        QVERIFY(light.turning());
        QVERIFY(apart(light.angle(), kodosi::light::phase(QDateTime::currentMSecsSinceEpoch())) < 15);
        light.setProperty("still", true);
        QVERIFY(!light.turning());
        QCOMPARE(light.angle(), -45.0);
        light.setProperty("still", false);
        QVERIFY(light.turning());
        light.setVisible(false);
        QVERIFY(!light.turning());
        QCOMPARE(light.angle(), -45.0);

        Mesh tile({ QSizeF(44, 44), 13.2, 3.08, 1, true });
        tile.paint({ .angle = light.angle(), .light = tileLight });
        const auto& vertices = tile.vertices();
        const auto brightest
            = std::max_element(vertices.begin(), vertices.end(), [](const auto& a, const auto& b) { return a.a < b.a; });
        QCOMPARE(int { brightest->a }, 255);
        QVERIFY(apart(angleOf(tile.centre(), *brightest), 315) < 0.01);
        QVERIFY(brightest->b > 0.9 * 255);
        QVERIFY(std::any_of(vertices.begin(), vertices.end(),
            [](const auto& vertex) { return vertex.x > 8 && vertex.x < 16 && vertex.y > 8 && vertex.y < 16 && vertex.a > 0; }));
        QVERIFY(std::all_of(vertices.begin(), vertices.end(),
            [](const auto& vertex) { return vertex.x < 26 || vertex.y < 26 || vertex.a == 0; }));
    }

    void onlyATurningLightRunsTheClock()
    {
        kodosi::LightTrack alone;
        alone.setSize(QSizeF(30, 30));
        QVERIFY(!alone.turning());
        QVERIFY(!kodosi::LightTrack::clockRunning());

        QQuickWindow window;
        window.resize(120, 80);
        kodosi::LightTrack first(window.contentItem());
        kodosi::LightTrack second(window.contentItem());
        first.setSize(QSizeF(30, 30));
        second.setSize(QSizeF(30, 30));
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));
        QVERIFY(first.turning() && second.turning());
        QVERIFY(kodosi::LightTrack::clockRunning());
        const double before = first.angle();
        QTRY_VERIFY(first.angle() != before);
        QCOMPARE(first.angle(), second.angle());

        first.setProperty("mode", QStringLiteral("progress"));
        first.setProperty("fraction", 0.4);
        QVERIFY(!first.turning());
        QVERIFY(kodosi::LightTrack::clockRunning());
        second.setProperty("still", true);
        QVERIFY(!kodosi::LightTrack::clockRunning());
        second.setProperty("still", false);
        QVERIFY(kodosi::LightTrack::clockRunning());
        second.setVisible(false);
        QVERIFY(!kodosi::LightTrack::clockRunning());
    }

    void aRimRestsWhileItsWindowIsHiddenOrMotionIsReduced()
    {
        QTemporaryDir directory;
        kodosi::AppearanceModel appearance(
            std::make_unique<QSettings>(directory.filePath(QStringLiteral("appearance.ini")), QSettings::IniFormat),
            { [] { return Qt::ColorScheme::Light; }, [](Qt::ColorScheme) {}, [] {} });
        kodosi::qml::AppearanceModelForeign::instance = &appearance;
        QQmlEngine engine;
        QQmlComponent component(&engine);
        component.setData(R"(
            import QtQuick
            import Kodosi 1.0
            Window {
                width: 120; height: 80; visible: true
                LightRim { objectName: "ring"; width: 38; height: 38 }
                LightRim { objectName: "tile"; x: 50; width: 44; height: 44; tile: true }
            }
        )", {});
        std::unique_ptr<QObject> root(component.create());
        QVERIFY2(root, qPrintable(component.errorString()));
        auto* ring = root->findChild<kodosi::LightTrack*>(QStringLiteral("ring"));
        auto* tile = root->findChild<kodosi::LightTrack*>(QStringLiteral("tile"));
        QVERIFY(ring && tile);
        QCOMPARE(ring->property("light").value<QColor>(), QColor(0xb8, 0x5a, 0x26));
        QCOMPARE(tile->property("light").value<QColor>(), tileLight);
        QVERIFY(ring->turning() && tile->turning());

        auto* theme = engine.singletonInstance<QObject*>("Kodosi", "KodosiTheme");
        QVERIFY(theme);
        theme->setProperty("shown", false);
        QVERIFY(!ring->turning() && !tile->turning());
        QVERIFY(!kodosi::LightTrack::clockRunning());
        QCOMPARE(tile->angle(), -45.0);
        theme->setProperty("shown", true);
        QVERIFY(tile->turning());
        QVERIFY(appearance.setReducesMotion(true));
        QVERIFY(!ring->turning() && !tile->turning());
        QCOMPARE(tile->angle(), -45.0);
    }
};

QTEST_MAIN(LightTrackTest)
#include "tst_light_track.moc"
