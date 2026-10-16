#include "controls/CellTrack.hpp"

#include <QImage>
#include <QQuickWindow>
#include <QtTest/QTest>

#include <algorithm>
#include <cmath>
#include <numbers>

namespace {
using kodosi::cells::Ink;

int cellOf(std::int64_t stroke, int count)
{
    return static_cast<int>(((stroke % count) + count) % count);
}

QColor pixel(const QImage& image, const QRectF& rect)
{
    return image.pixelColor(static_cast<int>(rect.center().x()), static_cast<int>(rect.center().y()));
}
}

class CellTrackTest final : public QObject {
    Q_OBJECT
private slots:
    void strokesRepeatInBurstsOfQuickKeys()
    {
        double longest = 0;
        for (std::int64_t stroke = 1000; stroke < 1064; ++stroke) {
            const double at = kodosi::cells::strokeTime(stroke);
            QCOMPARE(kodosi::cells::strokeAt(at + 1e-9), stroke);
            QCOMPARE(kodosi::cells::strokeAt(at - 1e-9), stroke - 1);
            QCOMPARE(kodosi::cells::strokeTime(stroke + 64) - at, kodosi::cells::strokeTime(64) - kodosi::cells::strokeTime(0));
            const double gap = kodosi::cells::strokeTime(stroke + 1) - at;
            QVERIFY(gap >= 0.045 && gap <= 0.75);
            longest = std::max(longest, gap);
        }
        QVERIFY(longest >= 0.3);
    }

    void aStrokeLightsItsCellAndTheCellCools()
    {
        const int count = 40;
        const auto stroke = kodosi::cells::strokeAt(9000) + 1;
        const double struck = kodosi::cells::strokeTime(stroke);
        const int cell = cellOf(stroke, count);
        const auto at = [&](double age) { return kodosi::cells::typing(count, struck + age); };
        QCOMPARE(at(0.01).cursor, cell);
        QVERIFY(at(0.01).inks[cell] == (Ink { .strong = true, .mix = 0 }));
        QVERIFY(at(0.2).inks[cell] == (Ink { .mix = 0 }));
        QVERIFY(at(0.5).inks[cell] == (Ink { .mix = 0.45 }));
        QVERIFY(at(1).inks[cell] == (Ink { .mix = 0.72 }));
        QVERIFY(at(1.5).inks[cell] == Ink {});
    }

    void nothingChangesBeforeTheNextChange()
    {
        for (const int count : { 8, 18, 40 }) {
            double time = 5000.25;
            for (int wake = 0; wake < 200; ++wake) {
                const double next = kodosi::cells::nextTypingChange(count, time);
                QVERIFY(next > time);
                const auto frame = kodosi::cells::typing(count, time);
                for (double probe = time; probe < next - 1e-6; probe += 0.002)
                    QVERIFY(kodosi::cells::typing(count, probe) == frame);
                QVERIFY(kodosi::cells::typing(count, next + 1e-6) != frame);
                time = next + 1e-6;
            }
        }
    }

    void theRimRunsClockwiseFromTheTop()
    {
        const auto shape = kodosi::cells::rim(QSizeF(100, 60), 10, 2);
        QCOMPARE(shape.cells.size(), std::size_t { 59 });
        QCOMPARE(shape.cursors.size(), shape.cells.size());
        QCOMPARE(shape.cells.front().top(), 0.0);
        QVERIFY(shape.cells.front().left() > 49);
        double turned = -std::numbers::pi / 2;
        for (std::size_t index = 0; index < shape.cells.size(); ++index) {
            const auto& cell = shape.cells[index];
            QVERIFY(QRectF(0, 0, 100, 60).adjusted(-1, -1, 1, 1).contains(cell));
            QVERIFY(std::abs(cell.width() - cell.height()) < 1e-9 || std::min(cell.width(), cell.height()) == 2);
            const auto offset = shape.cursors[index].center() - cell.center();
            QVERIFY(std::abs(offset.x()) <= 1 && std::abs(offset.y()) <= 1);
            double angle = std::atan2(cell.center().y() - 30, cell.center().x() - 50);
            while (angle < turned - 0.5)
                angle += 2 * std::numbers::pi;
            QVERIFY(angle >= turned - 1e-9);
            turned = angle;
        }
    }

    void progressFillsUpToItsCursor()
    {
        const auto half = kodosi::cells::progress(10, 0.45, true);
        QVERIFY(half.inks[3] == (Ink { .mix = 0 }));
        QCOMPARE(half.inks[4].mix, 0.5);
        QVERIFY(half.inks[5] == Ink {});
        QCOMPARE(half.cursor, 4);
        QCOMPARE(kodosi::cells::progress(10, 0.45, false).cursor, -1);
        QCOMPARE(kodosi::cells::progress(10, 1, true).cursor, -1);
    }

    void itDrawsItsCellsAndRunsOnlyWhileItCanBeSeen()
    {
        QQuickWindow window;
        window.resize(120, 80);
        kodosi::CellTrack track(window.contentItem());
        track.setSize(QSizeF(100, 60));
        track.setProperty("radius", 12);
        track.setProperty("lineWidth", 4);
        track.setProperty("strong", QColor(Qt::red));
        track.setProperty("accent", QColor(Qt::green));
        track.setProperty("soft", QColor(Qt::blue));
        track.setProperty("still", true);
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));

        const auto& shape = track.shape();
        const auto image = window.grabWindow();
        QCOMPARE(pixel(image, shape.cursors.front()), QColor(Qt::red));
        QCOMPARE(pixel(image, shape.cells.back()), QColor(Qt::green));
        QCOMPARE(pixel(image, shape.cells[shape.cells.size() / 2]), QColor(Qt::blue));
        QVERIFY(!track.animating());

        track.setProperty("still", false);
        QVERIFY(track.animating());
        track.setProperty("paused", true);
        QVERIFY(!track.animating());
        track.setProperty("paused", false);
        QVERIFY(track.animating());
        track.setVisible(false);
        QVERIFY(!track.animating());
        track.setVisible(true);
        track.setProperty("mode", QStringLiteral("progress"));
        track.setProperty("fraction", 1.0);
        QVERIFY(!track.animating());
    }
};

QTEST_MAIN(CellTrackTest)
#include "tst_cell_track.moc"
