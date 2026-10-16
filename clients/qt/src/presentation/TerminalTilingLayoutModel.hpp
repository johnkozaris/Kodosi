#pragma once

#include "presentation/DesktopStateModel.hpp"

#include <QAbstractListModel>
#include <QHash>
#include <QRectF>
#include <QStringList>
#include <QVector>

namespace kodosi {

class TerminalTilingLayoutModel final : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(
        qreal viewportWidth
        READ viewportWidth
        WRITE setViewportWidth
        NOTIFY viewportChanged)
    Q_PROPERTY(
        qreal viewportHeight
        READ viewportHeight
        WRITE setViewportHeight
        NOTIFY viewportChanged)
    Q_PROPERTY(qreal contentHeight READ contentHeight NOTIFY geometryChanged)
    Q_PROPERTY(
        qreal accessibilityTextScale
        READ accessibilityTextScale
        WRITE setAccessibilityTextScale
        NOTIFY accessibilityTextScaleChanged)

public:
    enum class EntryType {
        Tile,
        Divider,
    };
    Q_ENUM(EntryType)

    enum Role {
        EntryTypeRole = Qt::UserRole + 1,
        StableIdRole,
        SessionIdRole,
        XRole,
        YRole,
        WidthRole,
        HeightRole,
        VisibleRole,
        OrientationRole,
        PercentageRole,
    };
    Q_ENUM(Role)

    explicit TerminalTilingLayoutModel(QObject* parent = nullptr);

    [[nodiscard]] int rowCount(const QModelIndex& parent = {}) const override;
    [[nodiscard]] QVariant data(const QModelIndex& index, int role) const override;
    [[nodiscard]] QHash<int, QByteArray> roleNames() const override;

    [[nodiscard]] qreal viewportWidth() const noexcept;
    [[nodiscard]] qreal viewportHeight() const noexcept;
    [[nodiscard]] qreal contentHeight() const noexcept;
    [[nodiscard]] int layoutRowCount() const noexcept;
    [[nodiscard]] int layoutColumnCount() const noexcept;
    [[nodiscard]] qreal accessibilityTextScale() const noexcept;
    [[nodiscard]] qreal minimumTileWidth() const noexcept;
    [[nodiscard]] qreal minimumTileHeight() const noexcept;

    void setViewportWidth(qreal width);
    void setViewportHeight(qreal height);
    void setStagedSessionIds(QStringList sessionIds);
    void setSelectedSessionId(QString sessionId);
    void setLayoutMode(DesktopStateModel::StageLayoutMode mode);
    void setAccessibilityTextScale(qreal scale);

    Q_INVOKABLE [[nodiscard]] qreal adjustDivider(
        const QString& stableId,
        qreal deltaPixels);

    [[nodiscard]] static qreal requiredContentHeight(
        int rowCount,
        qreal viewportHeight,
        qreal minimumTileHeight = 170);
    [[nodiscard]] static QPair<qreal, qreal> adjustedPair(
        qreal previous,
        qreal current,
        qreal deltaPixels,
        qreal total,
        qreal minimumPoints);
    [[nodiscard]] static QVector<qreal> clampedProportions(
        QVector<qreal> proportions,
        qreal minimum);

signals:
    void viewportChanged();
    void accessibilityTextScaleChanged();
    void geometryChanged();

private:
    struct Entry {
        EntryType type = EntryType::Tile;
        QString stableId;
        QString sessionId;
        qreal x = 0;
        qreal y = 0;
        qreal width = 0;
        qreal height = 0;
        qreal percentage = 0;
        Qt::Orientation orientation = Qt::Horizontal;
        int boundaryRow = -1;
        int boundaryColumn = -1;
        bool visible = false;

        bool operator==(const Entry&) const = default;
    };

    QVector<Entry> m_entries;
    QStringList m_stagedSessionIds;
    QString m_selectedSessionId;
    QVector<QVector<qreal>> m_columnProportions;
    QVector<qreal> m_rowProportions;
    QHash<QString, QRectF> m_lastGridTileGeometry;
    QString m_topologyKey;
    qreal m_viewportWidth = 0;
    qreal m_viewportHeight = 0;
    qreal m_contentHeight = 0;
    qreal m_accessibilityTextScale = 1;
    int m_layoutRowCount = 0;
    int m_layoutColumnCount = 0;
    DesktopStateModel::StageLayoutMode m_layoutMode =
        DesktopStateModel::StageLayoutMode::Grid;

    void rebuild();
    [[nodiscard]] QVector<QStringList> rowsFor(int columns) const;
    [[nodiscard]] QString topologyKey(int columns) const;
    void resetProportions(const QVector<QStringList>& rows);
    void clampProportions(const QVector<QStringList>& rows);
    void synchronizeEntries(QVector<Entry> entries);
    [[nodiscard]] QVector<Entry> buildEntries(
        const QVector<QStringList>& rows);
    [[nodiscard]] int adaptiveColumnCount(
        int sessionCount,
        qreal width,
        qreal height) const;
};

}
