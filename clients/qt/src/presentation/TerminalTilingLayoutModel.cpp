#include "presentation/TerminalTilingLayoutModel.hpp"

#include <QtNumeric>

#include <algorithm>
#include <cmath>
#include <limits>
#include <numeric>
#include <ranges>
#include <utility>

namespace kodosi {
namespace {

qreal layoutPenalty(
    const int columns,
    const int sessionCount,
    const qreal width,
    const qreal height,
    const qreal minimumTileWidth,
    const qreal minimumTileHeight)
{
    const auto rows = static_cast<int>(
        std::ceil(static_cast<qreal>(sessionCount) / columns));
    const auto cellWidth =
        (width - std::max(0, columns - 1) * 6.0) / columns;
    const auto cellHeight =
        (height - std::max(0, rows - 1) * 6.0) / rows;
    const auto aspectPenalty = std::abs(
        std::log(std::max(0.01, cellWidth / cellHeight) / 1.3));
    const auto widthPenalty =
        std::max(0.0, minimumTileWidth - cellWidth)
        / minimumTileWidth;
    const auto heightPenalty =
        std::max(0.0, minimumTileHeight - cellHeight)
        / minimumTileHeight;
    const auto emptySlots = rows * columns - sessionCount;
    const auto incompleteRowPenalty =
        static_cast<qreal>(emptySlots) / columns * 0.35;
    return aspectPenalty + widthPenalty * 1.5
        + heightPenalty * 1.5 + incompleteRowPenalty;
}

bool validExtent(const qreal value)
{
    return std::isfinite(value) && value >= 0;
}

}

TerminalTilingLayoutModel::TerminalTilingLayoutModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int TerminalTilingLayoutModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_entries.size();
}

QVariant TerminalTilingLayoutModel::data(
    const QModelIndex& index,
    const int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_entries.size()) {
        return {};
    }
    const auto& entry = m_entries.at(index.row());
    switch (role) {
    case EntryTypeRole:
        return QVariant::fromValue(entry.type);
    case StableIdRole:
        return entry.stableId;
    case SessionIdRole:
        return entry.sessionId;
    case XRole:
        return entry.x;
    case YRole:
        return entry.y;
    case WidthRole:
        return entry.width;
    case HeightRole:
        return entry.height;
    case VisibleRole:
        return entry.visible;
    case OrientationRole:
        return QVariant::fromValue(entry.orientation);
    case PercentageRole:
        return entry.percentage;
    default:
        return {};
    }
}

QHash<int, QByteArray> TerminalTilingLayoutModel::roleNames() const
{
    return {
        {EntryTypeRole, QByteArrayLiteral("entryType")},
        {StableIdRole, QByteArrayLiteral("stableId")},
        {SessionIdRole, QByteArrayLiteral("sessionId")},
        {XRole, QByteArrayLiteral("layoutX")},
        {YRole, QByteArrayLiteral("layoutY")},
        {WidthRole, QByteArrayLiteral("layoutWidth")},
        {HeightRole, QByteArrayLiteral("layoutHeight")},
        {VisibleRole, QByteArrayLiteral("entryVisible")},
        {OrientationRole, QByteArrayLiteral("orientation")},
        {PercentageRole, QByteArrayLiteral("percentage")},
    };
}

qreal TerminalTilingLayoutModel::viewportWidth() const noexcept
{
    return m_viewportWidth;
}

qreal TerminalTilingLayoutModel::viewportHeight() const noexcept
{
    return m_viewportHeight;
}

QStringList TerminalTilingLayoutModel::stagedSessionIds() const
{
    return m_stagedSessionIds;
}

QString TerminalTilingLayoutModel::selectedSessionId() const
{
    return m_selectedSessionId;
}

DesktopStateModel::StageLayoutMode
TerminalTilingLayoutModel::layoutMode() const noexcept
{
    return m_layoutMode;
}

qreal TerminalTilingLayoutModel::contentHeight() const noexcept
{
    return m_contentHeight;
}

int TerminalTilingLayoutModel::layoutRowCount() const noexcept
{
    return m_layoutRowCount;
}

int TerminalTilingLayoutModel::columnCount() const noexcept
{
    return m_columnCount;
}

qreal TerminalTilingLayoutModel::accessibilityTextScale() const noexcept
{
    return m_accessibilityTextScale;
}

qreal TerminalTilingLayoutModel::minimumTileWidth() const noexcept
{
    return baseMinimumTileWidth;
}

qreal TerminalTilingLayoutModel::minimumTileHeight() const noexcept
{
    return m_accessibilityTextScale >= accessibilityTextScaleThreshold
        ? accessibilityMinimumTileHeight
        : baseMinimumTileHeight;
}

void TerminalTilingLayoutModel::setViewportWidth(const qreal width)
{
    if (!validExtent(width) || qFuzzyCompare(m_viewportWidth, width)) {
        return;
    }
    m_viewportWidth = width;
    emit viewportChanged();
    rebuild();
}

void TerminalTilingLayoutModel::setViewportHeight(const qreal height)
{
    if (!validExtent(height) || qFuzzyCompare(m_viewportHeight, height)) {
        return;
    }
    m_viewportHeight = height;
    emit viewportChanged();
    rebuild();
}

void TerminalTilingLayoutModel::setStagedSessionIds(QStringList sessionIds)
{
    sessionIds.removeDuplicates();
    sessionIds.removeAll(QString {});
    if (m_stagedSessionIds == sessionIds) {
        return;
    }
    m_stagedSessionIds = std::move(sessionIds);
    m_lastGridTileGeometry.removeIf([this](const auto& entry) {
        return !m_stagedSessionIds.contains(entry.key());
    });
    emit stagedSessionIdsChanged();
    rebuild();
}

void TerminalTilingLayoutModel::setSelectedSessionId(QString sessionId)
{
    if (m_selectedSessionId == sessionId) {
        return;
    }
    m_selectedSessionId = std::move(sessionId);
    emit selectedSessionIdChanged();
    rebuild();
}

void TerminalTilingLayoutModel::setLayoutMode(
    const DesktopStateModel::StageLayoutMode mode)
{
    if (m_layoutMode == mode) {
        return;
    }
    m_layoutMode = mode;
    emit layoutModeChanged();
    rebuild();
}

void TerminalTilingLayoutModel::setAccessibilityTextScale(const qreal scale)
{
    if (!std::isfinite(scale)) {
        return;
    }
    const auto clamped = std::clamp(scale, 1.0, 2.0);
    if (qFuzzyCompare(m_accessibilityTextScale, clamped)) {
        return;
    }
    m_accessibilityTextScale = clamped;
    emit accessibilityTextScaleChanged();
    rebuild();
}

qreal TerminalTilingLayoutModel::adjustDivider(
    const QString& stableId,
    const qreal deltaPixels)
{
    if (!std::isfinite(deltaPixels) || qFuzzyIsNull(deltaPixels)) {
        return 0;
    }
    const auto found = std::ranges::find(
        m_entries,
        stableId,
        &Entry::stableId);
    if (found == m_entries.end()
        || found->type != EntryType::Divider
        || !found->visible) {
        return 0;
    }
    const auto usableWidth =
        std::max(0.0, m_viewportWidth - stageInset * 2);
    const auto usableViewportHeight =
        std::max(0.0, m_viewportHeight - stageInset * 2);
    const auto availableHeight = requiredContentHeight(
        m_layoutRowCount,
        usableViewportHeight,
        minimumTileHeight())
        - std::max(0, m_layoutRowCount - 1) * dividerSize;
    if (found->orientation == Qt::Horizontal) {
        const auto current = found->boundaryRow;
        if (current <= 0 || current >= m_rowProportions.size()) {
            return 0;
        }
        const auto previous = m_rowProportions.at(current - 1);
        const auto adjusted = adjustedPair(
            previous,
            m_rowProportions.at(current),
            deltaPixels,
            availableHeight,
            minimumTileHeight());
        m_rowProportions[current - 1] = adjusted.first;
        m_rowProportions[current] = adjusted.second;
        const auto accepted =
            (adjusted.first - previous) * availableHeight;
        rebuild();
        return accepted;
    } else {
        const auto row = found->boundaryRow;
        const auto current = found->boundaryColumn;
        if (row < 0 || row >= m_columnProportions.size()
            || current <= 0
            || current >= m_columnProportions.at(row).size()) {
            return 0;
        }
        const auto columnCount = static_cast<int>(
            m_columnProportions.at(row).size());
        const auto availableWidth =
            usableWidth - std::max(0, columnCount - 1) * dividerSize;
        const auto previous =
            m_columnProportions.at(row).at(current - 1);
        const auto adjusted = adjustedPair(
            previous,
            m_columnProportions.at(row).at(current),
            deltaPixels,
            availableWidth,
            minimumTileWidth());
        m_columnProportions[row][current - 1] = adjusted.first;
        m_columnProportions[row][current] = adjusted.second;
        const auto accepted =
            (adjusted.first - previous) * availableWidth;
        rebuild();
        return accepted;
    }
}

qreal TerminalTilingLayoutModel::requiredContentHeight(
    const int rowCount,
    const qreal viewportHeight,
    const qreal minimumTileHeight)
{
    if (rowCount <= 0) {
        return std::max(0.0, viewportHeight);
    }
    const auto minimum = rowCount * minimumTileHeight
        + std::max(0, rowCount - 1) * dividerSize;
    return std::max(viewportHeight, minimum);
}

QPair<qreal, qreal> TerminalTilingLayoutModel::adjustedPair(
    const qreal previous,
    const qreal current,
    const qreal deltaPixels,
    const qreal total,
    const qreal minimumPoints)
{
    if (total <= 0) {
        return {previous, current};
    }
    const auto pair = previous + current;
    const auto minimum =
        std::min(pair / 2, minimumPoints / total);
    const auto adjustedPrevious = std::clamp(
        previous + deltaPixels / total,
        minimum,
        pair - minimum);
    return {adjustedPrevious, pair - adjustedPrevious};
}

QVector<qreal> TerminalTilingLayoutModel::clampedProportions(
    QVector<qreal> proportions,
    const qreal minimum)
{
    if (proportions.isEmpty()) {
        return {};
    }
    const auto even = 1.0 / proportions.size();
    const auto floor = std::min(std::max(0.0, minimum), even);
    const auto sum = std::accumulate(
        proportions.cbegin(),
        proportions.cend(),
        0.0);
    if (sum <= 0) {
        return QVector<qreal>(proportions.size(), even);
    }
    for (auto& proportion : proportions) {
        proportion = std::max(floor, std::max(0.0, proportion) / sum);
    }
    const auto normalizedSum = std::accumulate(
        proportions.cbegin(),
        proportions.cend(),
        0.0);
    const auto excess = normalizedSum - 1.0;
    if (excess <= 0) {
        return proportions;
    }
    auto donorCapacity = 0.0;
    for (const auto proportion : std::as_const(proportions)) {
        donorCapacity += std::max(0.0, proportion - floor);
    }
    if (donorCapacity <= 0) {
        return QVector<qreal>(proportions.size(), even);
    }
    for (auto& proportion : proportions) {
        if (proportion > floor) {
            proportion -= excess
                * ((proportion - floor) / donorCapacity);
        }
    }
    return proportions;
}

void TerminalTilingLayoutModel::rebuild()
{
    const auto usableWidth =
        std::max(0.0, m_viewportWidth - stageInset * 2);
    const auto usableHeight =
        std::max(0.0, m_viewportHeight - stageInset * 2);
    const auto columns = m_stagedSessionIds.isEmpty()
        ? 0
        : adaptiveColumnCount(
              m_stagedSessionIds.size(),
              usableWidth,
              usableHeight);
    const auto rows = rowsFor(columns);
    const auto nextTopologyKey = topologyKey(columns);
    const auto topologyChanged = nextTopologyKey != m_topologyKey;
    if (topologyChanged) {
        m_topologyKey = nextTopologyKey;
        resetProportions(rows);
    } else {
        clampProportions(rows);
    }
    const auto nextEntries = buildEntries(rows);
    const auto nextContentHeight =
        m_layoutMode == DesktopStateModel::StageLayoutMode::Focus
        ? m_viewportHeight
        : rows.isEmpty()
        ? m_viewportHeight
        : requiredContentHeight(
              rows.size(),
              usableHeight,
              minimumTileHeight())
            + stageInset * 2;
    const auto geometrySummaryChanged =
        m_contentHeight != nextContentHeight
        || m_layoutRowCount != rows.size()
        || m_columnCount != columns;
    m_contentHeight = nextContentHeight;
    m_layoutRowCount = rows.size();
    m_columnCount = columns;

    synchronizeEntries(nextEntries);
    if (geometrySummaryChanged || !nextEntries.isEmpty()) {
        emit geometryChanged();
    }
}

QVector<QStringList> TerminalTilingLayoutModel::rowsFor(
    const int columns) const
{
    QVector<QStringList> rows;
    if (columns <= 0) {
        return rows;
    }
    for (qsizetype index = 0;
         index < m_stagedSessionIds.size();
         index += columns) {
        rows.append(
            m_stagedSessionIds.mid(index, columns));
    }
    return rows;
}

QString TerminalTilingLayoutModel::topologyKey(
    const int columns) const
{
    return QString::number(columns)
        + QLatin1Char(':')
        + m_stagedSessionIds.join(QChar(0x1f));
}

void TerminalTilingLayoutModel::resetProportions(
    const QVector<QStringList>& rows)
{
    m_rowProportions.clear();
    m_columnProportions.clear();
    if (rows.isEmpty()) {
        return;
    }
    m_rowProportions.fill(
        1.0 / rows.size(),
        rows.size());
    m_columnProportions.reserve(rows.size());
    for (const auto& row : rows) {
        QVector<qreal> proportions;
        proportions.fill(1.0 / row.size(), row.size());
        m_columnProportions.append(std::move(proportions));
    }
}

void TerminalTilingLayoutModel::clampProportions(
    const QVector<QStringList>& rows)
{
    if (rows.isEmpty()) {
        return;
    }
    const auto usableWidth =
        std::max(0.0, m_viewportWidth - stageInset * 2);
    const auto usableHeight =
        std::max(0.0, m_viewportHeight - stageInset * 2);
    const auto contentHeight =
        requiredContentHeight(
            rows.size(),
            usableHeight,
            minimumTileHeight());
    const auto availableHeight =
        contentHeight
        - std::max<qsizetype>(0, rows.size() - 1) * dividerSize;
    m_rowProportions = clampedProportions(
        m_rowProportions,
        availableHeight > 0
            ? minimumTileHeight() / availableHeight
            : 0);
    for (qsizetype row = 0; row < rows.size(); ++row) {
        const auto availableWidth =
            usableWidth
            - std::max<qsizetype>(
                0,
                rows.at(row).size() - 1)
                * dividerSize;
        m_columnProportions[row] = clampedProportions(
            m_columnProportions.at(row),
            availableWidth > 0
                ? minimumTileWidth() / availableWidth
                : 0);
    }
}

QVector<TerminalTilingLayoutModel::Entry>
TerminalTilingLayoutModel::buildEntries(
    const QVector<QStringList>& rows)
{
    QVector<Entry> result;
    if (rows.isEmpty()) {
        return result;
    }
    const auto focusMode =
        m_layoutMode == DesktopStateModel::StageLayoutMode::Focus;
    const auto usableWidth =
        std::max(0.0, m_viewportWidth - stageInset * 2);
    const auto usableViewportHeight =
        std::max(0.0, m_viewportHeight - stageInset * 2);
    const auto usableContentHeight =
        requiredContentHeight(
            rows.size(),
            usableViewportHeight,
            minimumTileHeight());
    const auto availableHeight =
        usableContentHeight
        - std::max<qsizetype>(0, rows.size() - 1) * dividerSize;

    qreal rowY = stageInset;
    for (qsizetype row = 0; row < rows.size(); ++row) {
        const auto rowHeight =
            availableHeight * m_rowProportions.at(row);
        const auto& rowSessions = rows.at(row);
        const auto availableWidth =
            usableWidth
            - std::max<qsizetype>(
                0,
                rowSessions.size() - 1)
                * dividerSize;
        qreal columnX = stageInset;
        for (qsizetype column = 0;
             column < rowSessions.size();
             ++column) {
            const auto sessionId = rowSessions.at(column);
            const auto columnWidth =
                availableWidth
                * m_columnProportions.at(row).at(column);
            Entry entry {
                .type = EntryType::Tile,
                .stableId = QStringLiteral("tile:") + sessionId,
                .sessionId = sessionId,
                .x = columnX,
                .y = rowY,
                .width = columnWidth,
                .height = rowHeight,
                .percentage = 0,
                .orientation = Qt::Horizontal,
                .boundaryRow = static_cast<int>(row),
                .boundaryColumn = static_cast<int>(column),
                .visible = !focusMode
                    || sessionId == m_selectedSessionId,
            };
            if (!focusMode) {
                m_lastGridTileGeometry.insert(
                    sessionId,
                    QRectF(entry.x, entry.y, entry.width, entry.height));
            } else if (sessionId == m_selectedSessionId) {
                entry.x = stageInset;
                entry.y = stageInset;
                entry.width = usableWidth;
                entry.height = usableViewportHeight;
            } else if (const auto retained =
                           m_lastGridTileGeometry.constFind(sessionId);
                       retained != m_lastGridTileGeometry.cend()) {
                entry.x = retained->x();
                entry.y = retained->y();
                entry.width = retained->width();
                entry.height = retained->height();
            }
            result.append(std::move(entry));
            columnX += columnWidth;
            if (column + 1 < rowSessions.size()) {
                columnX += dividerSize;
            }
        }
        rowY += rowHeight;
        if (row + 1 < rows.size()) {
            rowY += dividerSize;
        }
    }

    rowY = stageInset;
    for (qsizetype row = 1; row < rows.size(); ++row) {
        rowY += availableHeight * m_rowProportions.at(row - 1);
        result.append({
            .type = EntryType::Divider,
            .stableId =
                QStringLiteral("row.%1").arg(
                    static_cast<qlonglong>(row)),
            .sessionId = {},
            .x = stageInset,
            .y = rowY,
            .width = usableWidth,
            .height = dividerSize,
            .percentage =
                m_rowProportions.at(row - 1) * 100,
            .orientation = Qt::Horizontal,
            .boundaryRow = static_cast<int>(row),
            .boundaryColumn = -1,
            .visible = !focusMode,
        });
        rowY += dividerSize;
    }

    rowY = stageInset;
    for (qsizetype row = 0; row < rows.size(); ++row) {
        const auto rowHeight =
            availableHeight * m_rowProportions.at(row);
        const auto& rowSessions = rows.at(row);
        const auto availableWidth =
            usableWidth
            - std::max<qsizetype>(
                0,
                rowSessions.size() - 1)
                * dividerSize;
        qreal columnX = stageInset;
        for (qsizetype column = 1;
             column < rowSessions.size();
             ++column) {
            columnX += availableWidth
                * m_columnProportions.at(row).at(column - 1);
            result.append({
                .type = EntryType::Divider,
                .stableId = QStringLiteral(
                    "row.%1.column.%2")
                    .arg(static_cast<qlonglong>(row))
                    .arg(static_cast<qlonglong>(column)),
                .sessionId = {},
                .x = columnX,
                .y = rowY,
                .width = dividerSize,
                .height = rowHeight,
                .percentage =
                    m_columnProportions.at(row).at(column - 1)
                    * 100,
                .orientation = Qt::Vertical,
                .boundaryRow = static_cast<int>(row),
                .boundaryColumn = static_cast<int>(column),
                .visible = !focusMode,
            });
            columnX += dividerSize;
        }
        rowY += rowHeight;
        if (row + 1 < rows.size()) {
            rowY += dividerSize;
        }
    }
    return result;
}

int TerminalTilingLayoutModel::adaptiveColumnCount(
    const int sessionCount,
    const qreal width,
    const qreal height) const
{
    if (sessionCount <= 1 || width <= 0 || height <= 0) {
        return std::max(1, sessionCount);
    }
    if (width < singleColumnBreakpoint) {
        return 1;
    }
    const auto minimumWidth = minimumTileWidth();
    const auto columnsThatFit = std::max(
        1,
        static_cast<int>(
            (width + dividerSize)
            / (minimumWidth + dividerSize)));
    const auto maximumColumns =
        std::min(sessionCount, columnsThatFit);
    auto bestColumns = 1;
    auto bestPenalty = std::numeric_limits<qreal>::infinity();
    for (auto columns = 1; columns <= maximumColumns; ++columns) {
        const auto penalty = layoutPenalty(
            columns,
            sessionCount,
            width,
            height,
            minimumWidth,
            minimumTileHeight());
        if (penalty < bestPenalty) {
            bestPenalty = penalty;
            bestColumns = columns;
        }
    }
    return bestColumns;
}

void TerminalTilingLayoutModel::synchronizeEntries(QVector<Entry> entries)
{
    const auto nextTileCount = static_cast<int>(std::ranges::count(
        entries,
        EntryType::Tile,
        &Entry::type));
    auto currentTileCount = 0;
    while (currentTileCount < m_entries.size()
           && m_entries.at(currentTileCount).type == EntryType::Tile) {
        ++currentTileCount;
    }

    for (auto target = 0; target < nextTileCount; ++target) {
        const auto& desired = entries.at(target);
        auto source = target;
        while (source < currentTileCount
               && m_entries.at(source).stableId != desired.stableId) {
            ++source;
        }
        if (source == currentTileCount) {
            beginInsertRows({}, target, target);
            m_entries.insert(target, desired);
            endInsertRows();
            ++currentTileCount;
        } else if (source != target) {
            beginMoveRows({}, source, source, {}, target);
            m_entries.move(source, target);
            endMoveRows();
        }
        if (m_entries.at(target) != desired) {
            m_entries[target] = desired;
            emit dataChanged(index(target), index(target));
        }
    }
    if (currentTileCount > nextTileCount) {
        beginRemoveRows({}, nextTileCount, currentTileCount - 1);
        m_entries.remove(
            nextTileCount,
            currentTileCount - nextTileCount);
        endRemoveRows();
    }

    const auto nextDividerCount =
        entries.size() - nextTileCount;
    auto currentDividerCount =
        m_entries.size() - nextTileCount;
    for (auto divider = 0;
         divider < nextDividerCount;
         ++divider) {
        const auto target = nextTileCount + divider;
        const auto& desired = entries.at(target);
        auto source = target;
        while (source < nextTileCount + currentDividerCount
               && m_entries.at(source).stableId != desired.stableId) {
            ++source;
        }
        if (source == nextTileCount + currentDividerCount) {
            beginInsertRows({}, target, target);
            m_entries.insert(target, desired);
            endInsertRows();
            ++currentDividerCount;
        } else if (source != target) {
            beginMoveRows({}, source, source, {}, target);
            m_entries.move(source, target);
            endMoveRows();
        }
        if (m_entries.at(target) != desired) {
            m_entries[target] = desired;
            emit dataChanged(index(target), index(target));
        }
    }
    if (currentDividerCount > nextDividerCount) {
        const auto first = nextTileCount + nextDividerCount;
        const auto last =
            nextTileCount + currentDividerCount - 1;
        beginRemoveRows({}, first, last);
        m_entries.remove(
            first,
            currentDividerCount - nextDividerCount);
        endRemoveRows();
    }
}

}
