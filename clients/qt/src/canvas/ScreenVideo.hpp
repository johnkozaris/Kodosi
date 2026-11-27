#pragma once

#include "canvas/StreamVideo.hpp"

#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QVariantList>
#include <QVariantMap>

namespace kodosi {
// A computer's pointer as this device's cursor: its picture's size in pixels, its whole pixels per
// point, and its hotspot in points.
struct CursorPicture {
    QSize pixels;
    int scale = 1;
    QPoint hotspot;
};

// The cursor for a pointer of `points` with its hotspot, on a device of `deviceScale` pixels per
// point. Wayland takes a cursor only at a whole scale of one or more, whose multiple its size must
// be, and ends the app's connection otherwise; so the picture is scaled to whole points times the
// device's whole pixels per point, whatever the computer said.
[[nodiscard]] CursorPicture cursorPicture(QSizeF points, QPointF hotspot, qreal deviceScale);

// The screen of a terminal's computer on its canvas: the video of the display or window that it
// shows, its pointer, and the zoom of this view. Input and pointer positions are in the shown
// source's points. While another device drives the capture, the view shows what that device shows.
class ScreenVideo : public StreamVideo {
    Q_OBJECT
    Q_PROPERTY(QVariantList displays READ displays NOTIFY sourcesChanged FINAL)
    Q_PROPERTY(QVariantList windows READ windows NOTIFY sourcesChanged FINAL)
    Q_PROPERTY(QVariantMap source READ source NOTIFY sourceChanged FINAL)
    Q_PROPERTY(QVariantMap access READ access NOTIFY accessChanged FINAL)
    // Whether the person at the computer shares its screen, which the view watches only then.
    Q_PROPERTY(bool shared READ shared WRITE setShared FINAL)
    Q_PROPERTY(bool zoomed READ zoomed NOTIFY placed FINAL)
    // Whether the view shows what another device's view shows, until this one acts.
    Q_PROPERTY(bool follows READ follows NOTIFY placed FINAL)
    // The computer's pointer in this item's coordinates, with its picture, while it is over the source.
    // While the local pointer is over the item, it takes the same shape instead.
    Q_PROPERTY(QVariantMap computerPointer READ computerPointer NOTIFY computerPointerChanged FINAL)

public:
    explicit ScreenVideo(QQuickItem* parent = nullptr);
    ~ScreenVideo() override;

    [[nodiscard]] QVariantList displays() const { return m_displays; }
    [[nodiscard]] QVariantList windows() const { return m_windows; }
    [[nodiscard]] QVariantMap source() const { return m_source; }
    [[nodiscard]] QVariantMap access() const { return m_access; }
    [[nodiscard]] bool shared() const { return m_shared; }
    void setShared(bool shared);
    [[nodiscard]] bool zoomed() const { return m_zoom > 1; }
    [[nodiscard]] bool follows() const { return !m_control; }
    [[nodiscard]] QVariantMap computerPointer() const;

    // Zooms by `factor`, keeping the source point under (x, y) where it is.
    Q_INVOKABLE void zoom(qreal factor, qreal x, qreal y);
    // Moves a zoomed view by (dx, dy) in this item's coordinates, and tells whether it moved.
    Q_INVOKABLE bool pan(qreal dx, qreal dy);
    Q_INVOKABLE void showWhole();
    Q_INVOKABLE void show(const QString& kind, qint64 id);
    // Hands this device's clipboard to the computer ahead of the keys that paste, so they paste it there.
    Q_INVOKABLE void giveClipboard();

signals:
    void sourcesChanged();
    void sourceChanged();
    void accessChanged();
    void computerPointerChanged();
    void placed();
    // Text copied on the computer is on this device's clipboard now.
    void copied();

protected:
    void take(const QJsonObject& message) override;
    [[nodiscard]] std::optional<QPointF> streamPoint(QPointF point, bool clamped) const override;
    [[nodiscard]] QRectF pictureRect() const override;
    [[nodiscard]] bool takesInput() const override;
    [[nodiscard]] QString away() const override;
    [[nodiscard]] bool watchable() const override { return m_shared; }
    [[nodiscard]] QVariantMap kept() const override;
    void resume(const QVariantMap& kept) override;
    void forget() override;
    void seated() override;
    void settled() override;
    void moved() override;
    void acting() override;

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
    // The part of the source that this view's own zoom shows.
    [[nodiscard]] std::optional<QRectF> ownLook() const;
    [[nodiscard]] qreal fit() const;
    [[nodiscard]] std::optional<QRectF> shownRegion() const;
    [[nodiscard]] QPointF viewPoint(QPointF point, const QRectF& look) const;
    void takeControl();
    void looked();
    void sendLook();

    QVariantList m_displays;
    QVariantList m_windows;
    QVariantMap m_source;
    QVariantMap m_access;
    bool m_shared = true;
    // Why the computer's screen shows nothing, as the screen says, or nothing.
    QString m_away;
    // Whether the capture follows this view, which another device's view drives while not.
    bool m_control = true;
    // When this view acted and took the capture over, until the screen says so.
    QElapsedTimer m_claimed;
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
