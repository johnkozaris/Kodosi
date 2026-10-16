#pragma once

#include <QColor>
#include <QQuickItem>
#include <QRectF>
#include <QSizeF>
#include <QString>
#include <QTimer>
#include <QtQml/qqmlregistration.h>

#include <cstdint>
#include <vector>

namespace kodosi {

namespace cells {

struct Ink {
    bool strong = false;
    double mix = 1;

    static Ink cooling(double age);
    static Ink filling(double reach);
    friend bool operator==(const Ink&, const Ink&) = default;
};

struct Frame {
    std::vector<Ink> inks;
    int cursor = -1;

    friend bool operator==(const Frame&, const Frame&) = default;
};

struct Shape {
    std::vector<QRectF> cells;
    std::vector<QRectF> cursors;
    double cellRadius = 0;
    double cursorRadius = 0;
};

[[nodiscard]] std::int64_t strokeAt(double seconds);
[[nodiscard]] double strokeTime(std::int64_t stroke);

[[nodiscard]] Shape rim(QSizeF size, double radius, double lineWidth);
[[nodiscard]] Frame typing(int count, double seconds);
[[nodiscard]] Frame resting(int count);
[[nodiscard]] Frame progress(int count, double fraction, bool cursorLit);
[[nodiscard]] double nextTypingChange(int count, double seconds);

}

class CellTrack : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QString mode MEMBER m_mode NOTIFY modeChanged FINAL)
    Q_PROPERTY(double fraction MEMBER m_fraction NOTIFY fractionChanged FINAL)
    Q_PROPERTY(double radius MEMBER m_radius NOTIFY radiusChanged FINAL)
    Q_PROPERTY(double lineWidth MEMBER m_lineWidth NOTIFY lineWidthChanged FINAL)
    Q_PROPERTY(QColor strong MEMBER m_strong NOTIFY colorsChanged FINAL)
    Q_PROPERTY(QColor accent MEMBER m_accent NOTIFY colorsChanged FINAL)
    Q_PROPERTY(QColor soft MEMBER m_soft NOTIFY colorsChanged FINAL)
    Q_PROPERTY(bool still MEMBER m_still NOTIFY stillChanged FINAL)
    Q_PROPERTY(bool paused MEMBER m_paused NOTIFY pausedChanged FINAL)

public:
    explicit CellTrack(QQuickItem* parent = nullptr);

    [[nodiscard]] bool animating() const { return m_timer.isActive(); }
    [[nodiscard]] const cells::Frame& frame() const { return m_frame; }
    [[nodiscard]] const cells::Shape& shape() const { return m_shape; }

signals:
    void modeChanged();
    void fractionChanged();
    void radiusChanged();
    void lineWidthChanged();
    void colorsChanged();
    void stillChanged();
    void pausedChanged();

protected:
    void geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData& value) override;
    QSGNode* updatePaintNode(QSGNode* oldNode, UpdatePaintNodeData* data) override;

private:
    void reshape();
    void refresh();
    [[nodiscard]] QColor color(const cells::Ink& ink) const;

    QString m_mode = QStringLiteral("working");
    double m_fraction = 0;
    double m_radius = 8;
    double m_lineWidth = 2;
    QColor m_strong;
    QColor m_accent;
    QColor m_soft;
    bool m_still = false;
    bool m_paused = false;
    QTimer m_timer;
    cells::Shape m_shape;
    cells::Frame m_frame;
    bool m_shapeChanged = true;
    bool m_inksChanged = true;
};

}
