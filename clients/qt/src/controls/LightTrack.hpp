#pragma once

#include <QColor>
#include <QPointF>
#include <QQuickItem>
#include <QSGGeometry>
#include <QSizeF>
#include <QString>
#include <QtQml/qqmlregistration.h>

#include <array>
#include <cstdint>
#include <vector>

namespace kodosi {

namespace light {

inline constexpr double rest = -45;
inline constexpr int rays = 120;

[[nodiscard]] double glow(double degrees);
[[nodiscard]] double phase(std::int64_t milliseconds);

struct Shape {
    QSizeF size;
    double radius = -1;
    double lineWidth = 2;
    double pixel = 1;
    bool face = false;

    friend bool operator==(const Shape&, const Shape&) = default;
};

struct Look {
    bool progress = false;
    double fraction = 0;
    double angle = rest;
    QColor light;
};

struct Curve {
    std::vector<QPointF> points;
    std::vector<double> angles;
};

struct Section {
    double angle = 0;
    std::array<QPointF, 4> points;
};

class Mesh {
public:
    Mesh() = default;
    explicit Mesh(const Shape& shape);

    bool paint(const Look& look);

    [[nodiscard]] const Shape& shape() const { return m_shape; }
    [[nodiscard]] QPointF centre() const { return m_centre; }
    [[nodiscard]] const std::vector<QSGGeometry::ColoredPoint2D>& vertices() const { return m_vertices; }
    [[nodiscard]] const std::vector<quint16>& indices() const { return m_indices; }

private:
    enum class Layout { None, Tile, Ring, Progress };

    [[nodiscard]] Section section(double degrees) const;
    [[nodiscard]] double pixelAngle(const Section& section) const;
    void warm(const Look& look);
    void turn(const Look& look);
    void meter(const Look& look);
    void index();

    Shape m_shape { QSizeF(0, 0) };
    QPointF m_centre;
    std::array<Curve, 4> m_edges;
    std::vector<Section> m_ring;
    std::vector<std::array<QPointF, 2>> m_face;
    std::vector<QSGGeometry::ColoredPoint2D> m_vertices;
    std::vector<quint16> m_indices;
    Layout m_layout = Layout::None;
};

}

class LightClock;

class LightTrack : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QString mode MEMBER m_mode NOTIFY modeChanged FINAL)
    Q_PROPERTY(double fraction MEMBER m_fraction NOTIFY fractionChanged FINAL)
    Q_PROPERTY(double radius MEMBER m_radius NOTIFY radiusChanged FINAL)
    Q_PROPERTY(double lineWidth MEMBER m_lineWidth NOTIFY lineWidthChanged FINAL)
    Q_PROPERTY(bool tile MEMBER m_tile NOTIFY tileChanged FINAL)
    Q_PROPERTY(QColor light MEMBER m_light NOTIFY lightChanged FINAL)
    Q_PROPERTY(bool still MEMBER m_still NOTIFY stillChanged FINAL)

public:
    explicit LightTrack(QQuickItem* parent = nullptr);
    ~LightTrack() override;

    [[nodiscard]] double angle() const { return m_angle; }
    [[nodiscard]] bool turning() const { return m_turning; }
    [[nodiscard]] static bool clockRunning();

signals:
    void modeChanged();
    void fractionChanged();
    void radiusChanged();
    void lineWidthChanged();
    void tileChanged();
    void lightChanged();
    void stillChanged();

protected:
    void geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;
    QSGNode* updatePaintNode(QSGNode* oldNode, UpdatePaintNodeData* data) override;

private:
    friend class LightClock;

    void refresh();
    void turnTo(double angle);

    QString m_mode = QStringLiteral("working");
    double m_fraction = 0;
    double m_radius = -1;
    double m_lineWidth = 2;
    bool m_tile = false;
    QColor m_light = QColor(Qt::transparent);
    bool m_still = false;
    double m_angle = light::rest;
    bool m_turning = false;
    light::Mesh m_mesh;
};

}
