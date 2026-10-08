#include "presentation/TerminalTilingLayoutModel.hpp"

#include <QPersistentModelIndex>
#include <QSignalSpy>
#include <QtTest/QTest>

#include <cmath>

class TerminalTilingLayoutModelTest final : public QObject {
    Q_OBJECT

private slots:
    void laysOutOneThroughSixSessions_data();
    void laysOutOneThroughSixSessions();
    void choosesAdaptiveColumnsAndIncompleteRows();
    void scrollsWhenRowsCannotMeetMinimumHeight();
    void clampsMinimumTileSizes();
    void adjustsAndClampsDividers();
    void preservesProportionsAcrossViewportResize();
    void resetsProportionsOnlyForTopologyChanges();
    void focusKeepsAllTilesAliveAndHidesDividers();
    void preservesTileRowsAcrossMembershipAndBreakpoints();
    void accessibilityTextScaleKeepsMinimumWidthFixed();
    void accessibilityTextScaleRaisesMinimumHeight();
};

namespace {

QStringList sessionIds(const int count)
{
    QStringList result;
    for (auto index = 0; index < count; ++index) {
        result.append(QStringLiteral("session-%1").arg(index));
    }
    return result;
}

QVector<int> rowsForType(
    const kodosi::TerminalTilingLayoutModel& model,
    const kodosi::TerminalTilingLayoutModel::EntryType type)
{
    QVector<int> result;
    for (auto row = 0; row < model.rowCount(); ++row) {
        if (model.data(
                model.index(row),
                kodosi::TerminalTilingLayoutModel::EntryTypeRole)
                .value<kodosi::TerminalTilingLayoutModel::EntryType>()
            == type) {
            result.append(row);
        }
    }
    return result;
}

int rowForStableId(
    const kodosi::TerminalTilingLayoutModel& model,
    const QString& stableId)
{
    for (auto row = 0; row < model.rowCount(); ++row) {
        if (model.data(
                model.index(row),
                kodosi::TerminalTilingLayoutModel::StableIdRole)
                .toString()
            == stableId) {
            return row;
        }
    }
    return -1;
}

qreal roleReal(
    const kodosi::TerminalTilingLayoutModel& model,
    const int row,
    const int role)
{
    return model.data(model.index(row), role).toReal();
}

}

void TerminalTilingLayoutModelTest::laysOutOneThroughSixSessions_data()
{
    QTest::addColumn<int>("sessionCount");
    for (auto count = 1; count <= 6; ++count) {
        QTest::addRow("%d-sessions", count) << count;
    }
}

void TerminalTilingLayoutModelTest::laysOutOneThroughSixSessions()
{
    QFETCH(int, sessionCount);
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(sessionCount));

    const auto tiles = rowsForType(
        model,
        kodosi::TerminalTilingLayoutModel::EntryType::Tile);
    QCOMPARE(tiles.size(), sessionCount);
    QCOMPARE(
        model.layoutRowCount(),
        static_cast<int>(
            std::ceil(
                static_cast<qreal>(sessionCount)
                / model.columnCount())));
    for (const auto row : tiles) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::WidthRole)
            > 0);
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::HeightRole)
            > 0);
    }
}

void TerminalTilingLayoutModelTest::choosesAdaptiveColumnsAndIncompleteRows()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(5));
    QCOMPARE(model.columnCount(), 3);
    QCOMPARE(model.layoutRowCount(), 2);

    const auto lastTile = rowForStableId(
        model,
        QStringLiteral("tile:session-4"));
    QVERIFY(lastTile >= 0);
    QVERIFY(roleReal(
                model,
                lastTile,
                kodosi::TerminalTilingLayoutModel::WidthRole)
        > 300);

    model.setViewportWidth(589);
    QCOMPARE(model.columnCount(), 1);
    QCOMPARE(model.layoutRowCount(), 5);
}

void TerminalTilingLayoutModelTest::scrollsWhenRowsCannotMeetMinimumHeight()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(589);
    model.setViewportHeight(508);
    model.setStagedSessionIds(sessionIds(4));

    QCOMPARE(model.layoutRowCount(), 4);
    QCOMPARE(model.contentHeight(), 4 * 170 + 3 * 6 + 8);
    QVERIFY(model.contentHeight() > model.viewportHeight());
}

void TerminalTilingLayoutModelTest::clampsMinimumTileSizes()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(908);
    model.setViewportHeight(508);
    model.setStagedSessionIds(sessionIds(4));

    const auto tiles = rowsForType(
        model,
        kodosi::TerminalTilingLayoutModel::EntryType::Tile);
    for (const auto row : tiles) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::WidthRole)
            >= 280 - 0.01);
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::HeightRole)
            >= 170 - 0.01);
    }
}

void TerminalTilingLayoutModelTest::adjustsAndClampsDividers()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(4));
    const auto dividerRow = rowForStableId(
        model,
        QStringLiteral("row.0.column.1"));
    QVERIFY(dividerRow >= 0);
    const auto before = roleReal(
        model,
        dividerRow,
        kodosi::TerminalTilingLayoutModel::PercentageRole);

    QVERIFY(model.adjustDivider(
        QStringLiteral("row.0.column.1"),
        80));
    const auto after = roleReal(
        model,
        dividerRow,
        kodosi::TerminalTilingLayoutModel::PercentageRole);
    QVERIFY(after > before);

    const auto acceptedAtClamp = model.adjustDivider(
        QStringLiteral("row.0.column.1"),
        10'000);
    QVERIFY(acceptedAtClamp > 0);
    QVERIFY(acceptedAtClamp < 10'000);
    const auto firstTile = rowForStableId(
        model,
        QStringLiteral("tile:session-0"));
    const auto secondTile = rowForStableId(
        model,
        QStringLiteral("tile:session-1"));
    QVERIFY(roleReal(
                model,
                firstTile,
                kodosi::TerminalTilingLayoutModel::WidthRole)
        >= 280 - 0.01);
    QVERIFY(roleReal(
                model,
                secondTile,
                kodosi::TerminalTilingLayoutModel::WidthRole)
        >= 280 - 0.01);
}

void TerminalTilingLayoutModelTest::preservesProportionsAcrossViewportResize()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(4));
    QVERIFY(model.adjustDivider(
        QStringLiteral("row.0.column.1"),
        60));
    const auto dividerRow = rowForStableId(
        model,
        QStringLiteral("row.0.column.1"));
    const auto adjusted = roleReal(
        model,
        dividerRow,
        kodosi::TerminalTilingLayoutModel::PercentageRole);

    model.setViewportWidth(1'108);
    QCOMPARE(model.columnCount(), 2);
    const auto resized = roleReal(
        model,
        dividerRow,
        kodosi::TerminalTilingLayoutModel::PercentageRole);
    QVERIFY(std::abs(resized - adjusted) < 0.01);
}

void TerminalTilingLayoutModelTest::resetsProportionsOnlyForTopologyChanges()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(4));
    QVERIFY(model.adjustDivider(
        QStringLiteral("row.0.column.1"),
        60));
    auto dividerRow = rowForStableId(
        model,
        QStringLiteral("row.0.column.1"));
    QVERIFY(roleReal(
                model,
                dividerRow,
                kodosi::TerminalTilingLayoutModel::PercentageRole)
        > 50);

    auto changed = sessionIds(4);
    changed.swapItemsAt(0, 1);
    model.setStagedSessionIds(changed);
    dividerRow = rowForStableId(
        model,
        QStringLiteral("row.0.column.1"));
    QVERIFY(std::abs(
                roleReal(
                    model,
                    dividerRow,
                    kodosi::TerminalTilingLayoutModel::PercentageRole)
                - 50)
        < 0.01);
}

void TerminalTilingLayoutModelTest::
    focusKeepsAllTilesAliveAndHidesDividers()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(4));
    model.setSelectedSessionId(QStringLiteral("session-2"));
    const auto entryCount = model.rowCount();
    model.setLayoutMode(
        kodosi::DesktopStateModel::StageLayoutMode::Focus);

    QCOMPARE(model.rowCount(), entryCount);
    QCOMPARE(model.contentHeight(), model.viewportHeight());
    const auto tiles = rowsForType(
        model,
        kodosi::TerminalTilingLayoutModel::EntryType::Tile);
    auto visibleTiles = 0;
    for (const auto row : tiles) {
        visibleTiles += model.data(
            model.index(row),
            kodosi::TerminalTilingLayoutModel::VisibleRole).toBool();
    }
    QCOMPARE(visibleTiles, 1);
    const auto selectedTile = rowForStableId(
        model,
        QStringLiteral("tile:session-2"));
    QCOMPARE(
        roleReal(
            model,
            selectedTile,
            kodosi::TerminalTilingLayoutModel::WidthRole),
        1'000.0);
    for (const auto row : tiles) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::WidthRole)
            > 0);
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::HeightRole)
            > 0);
    }
    for (const auto row : rowsForType(
             model,
             kodosi::TerminalTilingLayoutModel::EntryType::Divider)) {
        QVERIFY(!model.data(
            model.index(row),
            kodosi::TerminalTilingLayoutModel::VisibleRole).toBool());
    }
}

void TerminalTilingLayoutModelTest::
    preservesTileRowsAcrossMembershipAndBreakpoints()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(708);
    model.setStagedSessionIds(sessionIds(3));
    QSignalSpy resets(&model, &QAbstractItemModel::modelReset);
    const QPersistentModelIndex first(
        model.index(rowForStableId(
            model,
            QStringLiteral("tile:session-0"))));
    const QPersistentModelIndex third(
        model.index(rowForStableId(
            model,
            QStringLiteral("tile:session-2"))));

    model.setStagedSessionIds(sessionIds(4));
    QVERIFY(first.isValid());
    QVERIFY(third.isValid());
    QCOMPARE(
        model.data(
            first,
            kodosi::TerminalTilingLayoutModel::StableIdRole)
            .toString(),
        QStringLiteral("tile:session-0"));
    QCOMPARE(
        model.data(
            third,
            kodosi::TerminalTilingLayoutModel::StableIdRole)
            .toString(),
        QStringLiteral("tile:session-2"));

    model.setViewportWidth(568);
    QCOMPARE(model.columnCount(), 1);
    QVERIFY(first.isValid());
    QVERIFY(third.isValid());

    auto reduced = sessionIds(4);
    reduced.removeAt(1);
    model.setStagedSessionIds(reduced);
    QVERIFY(first.isValid());
    QVERIFY(third.isValid());
    QCOMPARE(resets.count(), 0);
}

void TerminalTilingLayoutModelTest::
    accessibilityTextScaleKeepsMinimumWidthFixed()
{
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(1'008);
    model.setViewportHeight(400);
    model.setStagedSessionIds(sessionIds(3));
    QCOMPARE(model.minimumTileWidth(), 280.0);
    QCOMPARE(model.columnCount(), 3);

    model.setAccessibilityTextScale(1.5);
    QCOMPARE(model.minimumTileWidth(), 280.0);
    QCOMPARE(model.columnCount(), 3);
    for (const auto row : rowsForType(
             model,
             kodosi::TerminalTilingLayoutModel::EntryType::Tile)) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::WidthRole)
            >= 280 - 0.01);
    }
}

void TerminalTilingLayoutModelTest::
    accessibilityTextScaleRaisesMinimumHeight()
{
    QCOMPARE(
        kodosi::TerminalTilingLayoutModel::requiredContentHeight(2, 100),
        346.0);
    kodosi::TerminalTilingLayoutModel model;
    model.setViewportWidth(568);
    model.setViewportHeight(508);
    model.setStagedSessionIds(sessionIds(4));
    QCOMPARE(model.minimumTileHeight(), 170.0);
    QCOMPARE(model.contentHeight(), 706.0);

    model.setAccessibilityTextScale(1.29);
    QCOMPARE(model.minimumTileHeight(), 170.0);
    QCOMPARE(model.contentHeight(), 706.0);

    model.setAccessibilityTextScale(1.3);
    QCOMPARE(model.minimumTileHeight(), 240.0);
    QCOMPARE(model.contentHeight(), 986.0);
    for (const auto row : rowsForType(
             model,
             kodosi::TerminalTilingLayoutModel::EntryType::Tile)) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::HeightRole)
            >= 240 - 0.01);
    }
    QCOMPARE(
        model.adjustDivider(QStringLiteral("row.1"), 10'000),
        0.0);
    for (const auto row : rowsForType(
             model,
             kodosi::TerminalTilingLayoutModel::EntryType::Tile)) {
        QVERIFY(roleReal(
                    model,
                    row,
                    kodosi::TerminalTilingLayoutModel::HeightRole)
            >= 240 - 0.01);
    }
}

QTEST_MAIN(TerminalTilingLayoutModelTest)

#include "tst_terminal_tiling_layout_model.moc"
