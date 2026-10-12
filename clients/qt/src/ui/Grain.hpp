#pragma once

#include <QImage>
#include <QQuickItem>

namespace kodosi {

/// The grain of the Kodosi ground: the Mac client's noise tile, one texel per
/// device pixel. Dark grain lightens like a screen blend, light grain darkens
/// like a multiply blend; the item's opacity sets the strength.
class Grain : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY darkChanged)

public:
    explicit Grain(QQuickItem* parent = nullptr);

    [[nodiscard]] bool dark() const noexcept;
    void setDark(bool dark);

    static constexpr int tileSize = 128;
    [[nodiscard]] static QImage tile(bool dark);

signals:
    void darkChanged();

protected:
    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;

private:
    bool m_dark = true;
};

}
