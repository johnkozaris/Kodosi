#pragma once

#include <QColor>
#include <QImage>
#include <QQuickItem>
#include <QSizeF>

namespace kodosi {

/// The Gaussian shadow of a rounded rectangle the size of the item. It spreads
/// three sigma past the item and draws through image nodes, so it renders the
/// same with and without a GPU.
class SoftShadow : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(qreal radius READ radius WRITE setRadius NOTIFY radiusChanged)
    Q_PROPERTY(qreal blur READ blur WRITE setBlur NOTIFY blurChanged)
    Q_PROPERTY(QColor color READ color WRITE setColor NOTIFY colorChanged)

public:
    explicit SoftShadow(QQuickItem* parent = nullptr);

    [[nodiscard]] qreal radius() const noexcept;
    void setRadius(qreal radius);
    [[nodiscard]] qreal blur() const noexcept;
    void setBlur(qreal blur);
    [[nodiscard]] QColor color() const;
    void setColor(const QColor& color);

    /// How far the shadow of `blur` reaches past its shape.
    [[nodiscard]] static qreal reach(qreal blur) noexcept;
    /// The shadow of a `shape` sized rounded rectangle, `reach(blur)` from each
    /// edge of the image, at `scale` image pixels per item pixel.
    [[nodiscard]] static QImage render(QSizeF shape, qreal radius, qreal blur, qreal scale, const QColor& color);

signals:
    void radiusChanged();
    void blurChanged();
    void colorChanged();

protected:
    QSGNode* updatePaintNode(QSGNode* node, UpdatePaintNodeData* data) override;
    void geometryChange(const QRectF& next, const QRectF& previous) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;

private:
    qreal m_radius = 0;
    qreal m_blur = 0;
    QColor m_color = Qt::black;
};

}
