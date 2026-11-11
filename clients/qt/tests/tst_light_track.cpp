#include "app/QmlModelTypes.hpp"
#include "controls/LightTrack.hpp"

#include <QColor>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QSettings>
#include <QTemporaryDir>
#include <QtQml/QQmlExtensionPlugin>
#include <QtTest/QTest>

#include <memory>

Q_IMPORT_QML_PLUGIN(KodosiPlugin)

class LightTrackTest final : public QObject {
    Q_OBJECT
private slots:
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

    void aRimHoldsStillWhileItsWindowIsHiddenOrMotionIsReduced()
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
        QCOMPARE(tile->property("light").value<QColor>(), QColor(0xea, 0xa4, 0x7e));
        QVERIFY(ring->turning() && tile->turning());

        auto* theme = engine.singletonInstance<QObject*>("Kodosi", "KodosiTheme");
        QVERIFY(theme);
        theme->setProperty("shown", false);
        QVERIFY(!ring->turning() && !tile->turning());
        QVERIFY(!kodosi::LightTrack::clockRunning());
        QVERIFY(tile->property("still").toBool());
        theme->setProperty("shown", true);
        QVERIFY(tile->turning());
        QVERIFY(appearance.setReducesMotion(true));
        QVERIFY(!ring->turning() && !tile->turning());
        QVERIFY(tile->property("still").toBool());
    }
};

QTEST_MAIN(LightTrackTest)
#include "tst_light_track.moc"
