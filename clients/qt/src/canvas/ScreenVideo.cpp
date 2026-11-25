#include "canvas/ScreenVideo.hpp"

#include <QCursor>
#include <QJsonArray>
#include <QPixmap>

#include <algorithm>
#include <cmath>
#include <utility>

namespace kodosi {
namespace {
// How often a zoom or a pan that goes on moves the capture to what the view shows, in milliseconds.
constexpr int lookEvery = 200;
// The closest view, in view points per source point.
constexpr qreal closest = 4;
// The smallest pointer, in view points per source point.
constexpr qreal smallestPointer = 0.6;

// Shape ids are 64-bit numbers, which JSON numbers carry as doubles on both sides of the lookup.
QString shapeKey(const QVariant& id)
{
    return QString::number(id.toDouble(), 'g', 17);
}
}

ScreenVideo::ScreenVideo(QQuickItem* parent)
    : StreamVideo(QStringLiteral("screen"), parent)
{
    m_looking.setSingleShot(true);
    connect(&m_looking, &QTimer::timeout, this, &ScreenVideo::sendLook);
}

void ScreenVideo::take(const QJsonObject& message)
{
    if (message.contains(QStringLiteral("sources"))) {
        const auto sources = message.value(QStringLiteral("sources")).toObject();
        m_displays = sources.value(QStringLiteral("displays")).toArray().toVariantList();
        m_windows = sources.value(QStringLiteral("windows")).toArray().toVariantList();
        emit sourcesChanged();
    }
    // The screen shows another source whole to every view.
    if (const auto shown = message.value(QStringLiteral("source")).toObject().toVariantMap(); shown != m_source) {
        m_source = shown;
        m_zoom = 1;
        m_sentLook.reset();
        emit sourceChanged();
        emit placed();
    }
    if (const QSizeF measured(message.value(QStringLiteral("width")).toDouble(), message.value(QStringLiteral("height")).toDouble());
        measured != m_sourceSize) {
        m_sourceSize = measured;
        emit placed();
        update();
    }
    const auto shapes = message.value(QStringLiteral("shapes")).toArray();
    for (const auto& value : shapes)
        takeShape(value.toObject().toVariantMap());
    if (const auto at = message.value(QStringLiteral("pointer")).toObject().toVariantMap(); at != m_pointerAt || !shapes.isEmpty()) {
        m_pointerAt = at;
        emit computerPointerChanged();
    }
    if (const auto access = message.value(QStringLiteral("access")).toObject().toVariantMap(); access != m_access) {
        m_access = access;
        emit accessChanged();
    }
    followShape();
}

void ScreenVideo::takeShape(const QVariantMap& shape)
{
    const auto png = shape.value(QStringLiteral("png")).toString();
    const auto picture = QImage::fromData(QByteArray::fromBase64(png.toLatin1()), "PNG");
    if (picture.isNull())
        return;
    m_shapes.insert(shapeKey(shape.value(QStringLiteral("id"))),
        { QStringLiteral("data:image/png;base64,") + png, picture,
            QPointF(shape.value(QStringLiteral("x")).toDouble(), shape.value(QStringLiteral("y")).toDouble()),
            QSizeF(shape.value(QStringLiteral("width")).toDouble(), shape.value(QStringLiteral("height")).toDouble()) });
}

// The local pointer over the item takes the shape that the computer's pointer has there, while it
// moves that pointer.
void ScreenVideo::followShape()
{
    if (!takesInput()) {
        if (!std::exchange(m_cursorShape, {}).isEmpty())
            unsetCursor();
        return;
    }
    const auto key = shapeKey(m_pointerAt.value(QStringLiteral("shape")));
    const auto found = m_shapes.constFind(key);
    if (m_pointerAt.isEmpty() || key == m_cursorShape || found == m_shapes.cend() || found->size.width() <= 0)
        return;
    m_cursorShape = key;
    auto pixmap = QPixmap::fromImage(found->picture);
    pixmap.setDevicePixelRatio(found->picture.width() / found->size.width());
    setCursor(QCursor(pixmap, qRound(found->hotspot.x()), qRound(found->hotspot.y())));
}

void ScreenVideo::forget()
{
    m_displays.clear();
    m_windows.clear();
    m_source.clear();
    m_access.clear();
    m_sourceSize = {};
    m_pointerAt.clear();
    m_shapes.clear();
    m_cursorShape.clear();
    unsetCursor();
    m_zoom = 1;
    m_sentLook.reset();
    emit sourcesChanged();
    emit sourceChanged();
    emit accessChanged();
    emit computerPointerChanged();
    emit placed();
}

// Without Accessibility, Kodosi Screen cannot play input, and the view still zooms.
bool ScreenVideo::takesInput() const
{
    return !m_access.contains(QStringLiteral("input")) || m_access.value(QStringLiteral("input")).toBool();
}

// A new pipe is a new seat at the screen, which shows the whole source.
void ScreenVideo::seated()
{
    m_sentLook.reset();
}

void ScreenVideo::watched()
{
    sendLook();
}

void ScreenVideo::settled()
{
    emit placed();
    emit computerPointerChanged();
    sendLook();
}

qreal ScreenVideo::fit() const
{
    if (m_sourceSize.isEmpty() || width() <= 0 || height() <= 0)
        return 0;
    return std::min(width() / m_sourceSize.width(), height() / m_sourceSize.height());
}

std::optional<QRectF> ScreenVideo::look() const
{
    const auto whole = fit();
    if (whole <= 0)
        return std::nullopt;
    if (m_zoom <= 1)
        return QRectF(QPointF(), m_sourceSize);
    const auto zoomed = whole * m_zoom;
    const auto lookWidth = std::min(width() / zoomed, m_sourceSize.width());
    const auto lookHeight = std::min(height() / zoomed, m_sourceSize.height());
    return QRectF(std::clamp(m_center.x() - lookWidth / 2, 0.0, m_sourceSize.width() - lookWidth),
        std::clamp(m_center.y() - lookHeight / 2, 0.0, m_sourceSize.height() - lookHeight), lookWidth, lookHeight);
}

QPointF ScreenVideo::viewPoint(QPointF point, const QRectF& look) const
{
    const auto scale = std::min(width() / look.width(), height() / look.height());
    return { (width() - look.width() * scale) / 2 + (point.x() - look.x()) * scale,
        (height() - look.height() * scale) / 2 + (point.y() - look.y()) * scale };
}

std::optional<QPointF> ScreenVideo::streamPoint(QPointF point, bool clamped) const
{
    const auto shown = look();
    if (!shown)
        return std::nullopt;
    const auto scale = std::min(width() / shown->width(), height() / shown->height());
    auto x = shown->x() + (point.x() - (width() - shown->width() * scale) / 2) / scale;
    auto y = shown->y() + (point.y() - (height() - shown->height() * scale) / 2) / scale;
    if (clamped) {
        x = std::clamp(x, 0.0, std::nextafter(m_sourceSize.width(), 0.0));
        y = std::clamp(y, 0.0, std::nextafter(m_sourceSize.height(), 0.0));
    }
    if (x < 0 || y < 0 || x >= m_sourceSize.width() || y >= m_sourceSize.height())
        return std::nullopt;
    return QPointF(x, y);
}

// The picture of a part of the source shows where that part is, so a zoom moves and scales the
// newest picture at once, before the screen sends the part that the view shows.
QRectF ScreenVideo::pictureRect() const
{
    const auto shown = look();
    if (!shown)
        return StreamVideo::pictureRect();
    const auto region = pictureRegion().value_or(QRectF(QPointF(), m_sourceSize));
    const auto scale = std::min(width() / shown->width(), height() / shown->height());
    return { viewPoint(region.topLeft(), *shown), region.size() * scale };
}

QVariantMap ScreenVideo::computerPointer() const
{
    const auto shown = look();
    const auto shape = m_shapes.constFind(shapeKey(m_pointerAt.value(QStringLiteral("shape"))));
    if (!shown || m_pointerAt.isEmpty() || shape == m_shapes.cend())
        return {};
    const auto scale = std::max(std::min(width() / shown->width(), height() / shown->height()), smallestPointer);
    const auto at = viewPoint(
        QPointF(m_pointerAt.value(QStringLiteral("x")).toDouble(), m_pointerAt.value(QStringLiteral("y")).toDouble()), *shown);
    return { { QStringLiteral("x"), at.x() - shape->hotspot.x() * scale }, { QStringLiteral("y"), at.y() - shape->hotspot.y() * scale },
        { QStringLiteral("width"), shape->size.width() * scale }, { QStringLiteral("height"), shape->size.height() * scale },
        { QStringLiteral("image"), shape->address } };
}

void ScreenVideo::zoom(qreal factor, qreal x, qreal y)
{
    const auto held = streamPoint(QPointF(x, y), true);
    const auto whole = fit();
    if (!held || whole <= 0)
        return;
    const auto next = std::clamp(m_zoom * factor, 1.0, std::max(1.0, closest / whole));
    if (next == m_zoom)
        return;
    m_zoom = next;
    const auto zoomed = whole * next;
    m_center = QPointF(held->x() - (x - width() / 2) / zoomed, held->y() - (y - height() / 2) / zoomed);
    looked();
}

bool ScreenVideo::pan(qreal dx, qreal dy)
{
    const auto whole = fit();
    const auto before = look();
    if (m_zoom <= 1 || !before)
        return false;
    m_center += QPointF(dx, dy) / (whole * m_zoom);
    if (look() == before) {
        m_center = before->center();
        return false;
    }
    looked();
    return true;
}

void ScreenVideo::showWhole()
{
    if (m_zoom <= 1)
        return;
    m_zoom = 1;
    looked();
}

void ScreenVideo::show(const QString& kind, qint64 id)
{
    post({ { QStringLiteral("type"), QStringLiteral("show") },
        { QStringLiteral("source"), QJsonObject { { QStringLiteral("kind"), kind }, { QStringLiteral("id"), id } } } });
}

// Keeps the middle where the source is, so that a pan back starts at once.
void ScreenVideo::looked()
{
    if (const auto shown = look())
        m_center = shown->center();
    emit placed();
    emit computerPointerChanged();
    update();
    sendLook();
}

// Sends what the view shows at most every `lookEvery`, and the last of a zoom or pan once it ends.
// Each look names its source, so the screen leaves one that comes after another source took its
// place.
void ScreenVideo::sendLook()
{
    if (!watching() || m_source.isEmpty())
        return;
    const auto shown = m_zoom > 1 ? look() : std::nullopt;
    if (shown == m_sentLook)
        return;
    if (m_lookSent.isValid() && m_lookSent.elapsed() < lookEvery) {
        m_looking.start(lookEvery - static_cast<int>(m_lookSent.elapsed()));
        return;
    }
    m_looking.stop();
    m_sentLook = shown;
    m_lookSent.start();
    post({ { QStringLiteral("type"), QStringLiteral("look") }, { QStringLiteral("source"), QJsonObject::fromVariantMap(m_source) },
        { QStringLiteral("region"),
            shown ? QJsonValue(QJsonObject { { QStringLiteral("x"), shown->x() }, { QStringLiteral("y"), shown->y() },
                        { QStringLiteral("width"), shown->width() }, { QStringLiteral("height"), shown->height() } })
                  : QJsonValue(QJsonValue::Null) } });
}
}
