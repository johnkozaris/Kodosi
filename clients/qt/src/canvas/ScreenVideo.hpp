#pragma once

#include "canvas/StreamVideo.hpp"

#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QVariantList>
#include <QVariantMap>

namespace kodosi {
// The screen of a terminal's computer on its canvas: the video of the display or window that it
// shows, its pointer, and the zoom of this view. Input and pointer positions are in the shown
// source's points.
class ScreenVideo : public StreamVideo {
    Q_OBJECT
    Q_PROPERTY(QVariantList displays READ displays NOTIFY sourcesChanged FINAL)
    Q_PROPERTY(QVariantList windows READ windows NOTIFY sourcesChanged FINAL)
    Q_PROPERTY(QVariantMap source READ source NOTIFY sourceChanged FINAL)
    Q_PROPERTY(QVariantMap access READ access NOTIFY accessChanged FINAL)
    Q_PROPERTY(bool zoomed READ zoomed NOTIFY placed FINAL)
    // The computer's pointer in this item's coordinates, with its picture, while it is over the source.
    // While the local pointer is over the item, it takes the same shape instead.
    Q_PROPERTY(QVariantMap computerPointer READ computerPointer NOTIFY computerPointerChanged FINAL)

public:
    explicit ScreenVideo(QQuickItem* parent = nullptr);

    [[nodiscard]] QVariantList displays() const { return m_displays; }
    [[nodiscard]] QVariantList windows() const { return m_windows; }
    [[nodiscard]] QVariantMap source() const { return m_source; }
    [[nodiscard]] QVariantMap access() const { return m_access; }
    [[nodiscard]] bool zoomed() const { return m_zoom > 1; }
    [[nodiscard]] QVariantMap computerPointer() const;

    // Zooms by `factor`, keeping the source point under (x, y) where it is.
    Q_INVOKABLE void zoom(qreal factor, qreal x, qreal y);
    // Moves a zoomed view by (dx, dy) in this item's coordinates, and tells whether it moved.
    Q_INVOKABLE bool pan(qreal dx, qreal dy);
    Q_INVOKABLE void showWhole();
    Q_INVOKABLE void show(const QString& kind, qint64 id);

signals:
    void sourcesChanged();
    void sourceChanged();
    void accessChanged();
    void computerPointerChanged();
    void placed();

protected:
    void take(const QJsonObject& message) override;
    [[nodiscard]] std::optional<QPointF> streamPoint(QPointF point, bool clamped) const override;
    [[nodiscard]] QRectF pictureRect() const override;
    [[nodiscard]] bool takesInput() const override;
    void forget() override;
    void seated() override;
    void watched() override;
    void settled() override;

private:
    // A pointer's picture with its hotspot and size, in points.
    struct Shape {
        QString address;
        QImage picture;
        QPointF hotspot;
        QSizeF size;
    };

    void takeShape(const QVariantMap& shape);
    void followShape();
    // The part of the source that the view shows, fit and centred, in the source's points.
    [[nodiscard]] std::optional<QRectF> look() const;
    [[nodiscard]] qreal fit() const;
    [[nodiscard]] QPointF viewPoint(QPointF point, const QRectF& look) const;
    void looked();
    void sendLook();

    QVariantList m_displays;
    QVariantList m_windows;
    QVariantMap m_source;
    QVariantMap m_access;
    QSizeF m_sourceSize;
    QVariantMap m_pointerAt;
    QHash<QString, Shape> m_shapes;
    QString m_cursorShape;
    qreal m_zoom = 1;
    QPointF m_center;
    // The part of the source that the screen captures for this view, or nothing for all of it.
    std::optional<QRectF> m_sentLook;
    QElapsedTimer m_lookSent;
    QTimer m_looking;
};
}
