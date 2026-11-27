#include "canvas/ScreenVideo.hpp"

#include <QBuffer>
#include <QClipboard>
#include <QCursor>
#include <QGuiApplication>
#include <QImageReader>
#include <QJsonArray>
#include <QPixmap>
#include <QQuickWindow>

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
// The largest pointer picture, in pixels and in points, as Kodosi Screen sends it at most.
constexpr int largestPointer = 256;
// The pointer pictures that a view keeps at most.
constexpr qsizetype mostShapes = 256;
// The most pixels per point that a cursor takes on this device.
constexpr int mostCursorScale = 4;
constexpr qsizetype mostClipboardBytes = 1024 * 1024;
// How long a state that the screen sent before it heard this view act may come after the act.
constexpr qint64 claimWait = 1000;

// Shape ids are 64-bit numbers, which JSON numbers carry as doubles on both sides of the lookup.
QString shapeKey(const QVariant& id)
{
    return QString::number(id.toDouble(), 'g', 17);
}

// A size that the computer tells, which a view places only when both sides are numbers above nothing.
QSizeF measured(qreal width, qreal height)
{
    return std::isfinite(width) && std::isfinite(height) && width > 0 && height > 0 ? QSizeF(width, height) : QSizeF();
}

QString awayReason(const QString& away)
{
    static const QStringList reasons { QStringLiteral("locked"), QStringLiteral("anotherAccount"), QStringLiteral("asleep") };
    return reasons.contains(away) ? away : QString();
}
}

CursorPicture cursorPicture(QSizeF points, QPointF hotspot, qreal deviceScale)
{
    const auto whole = [](qreal value, qreal least, qreal most) {
        return std::isfinite(value) ? static_cast<int>(std::clamp(std::round(value), least, most)) : static_cast<int>(least);
    };
    const auto scale = whole(std::ceil(deviceScale), 1, mostCursorScale);
    const QSize size(whole(points.width(), 1, largestPointer), whole(points.height(), 1, largestPointer));
    return { size * scale, scale,
        QPoint(whole(hotspot.x(), 0, size.width() - 1), whole(hotspot.y(), 0, size.height() - 1)) };
}

ScreenVideo::ScreenVideo(QQuickItem* parent)
    : StreamVideo(QStringLiteral("screen"), parent)
{
    m_looking.setSingleShot(true);
    connect(&m_looking, &QTimer::timeout, this, &ScreenVideo::sendLook);
}

ScreenVideo::~ScreenVideo()
{
    keep();
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
    if (const auto size = measured(message.value(QStringLiteral("width")).toDouble(), message.value(QStringLiteral("height")).toDouble());
        size != m_sourceSize) {
        m_sourceSize = size;
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
    // An away screen sends no pictures, so the last one is not live when it comes back.
    m_away = awayReason(message.value(QStringLiteral("away")).toString());
    if (!m_away.isEmpty())
        stale();
    // A view that acted drives the capture, and a state that the screen sent before it heard that
    // does not take the capture back from it.
    if (const auto control = message.value(QStringLiteral("control")).toBool(true);
        control || !m_claimed.isValid() || m_claimed.hasExpired(claimWait)) {
        m_claimed.invalidate();
        if (control != m_control) {
            m_control = control;
            emit placed();
            emit computerPointerChanged();
            update();
        }
    }
    if (const auto text = message.value(QStringLiteral("copied")).toString(); !text.isEmpty()) {
        QGuiApplication::clipboard()->setText(text);
        emit copied();
    }
    followShape();
    sendLook();
    restate();
}

// A shape that no pointer can have stays out, so the view keeps its own cursor for it.
void ScreenVideo::takeShape(const QVariantMap& shape)
{
    const auto key = shapeKey(shape.value(QStringLiteral("id")));
    const auto size = measured(shape.value(QStringLiteral("width")).toDouble(), shape.value(QStringLiteral("height")).toDouble());
    const QPointF hotspot(shape.value(QStringLiteral("x")).toDouble(), shape.value(QStringLiteral("y")).toDouble());
    if ((m_shapes.size() >= mostShapes && !m_shapes.contains(key)) || size.isEmpty() || !std::isfinite(hotspot.x())
        || !std::isfinite(hotspot.y()))
        return;
    const auto png = shape.value(QStringLiteral("png")).toString();
    auto bytes = QByteArray::fromBase64(png.toLatin1());
    QBuffer buffer(&bytes);
    QImageReader reader(&buffer, "PNG");
    const auto pixels = reader.size();
    if (!pixels.isValid() || pixels.width() > largestPointer || pixels.height() > largestPointer)
        return;
    const auto picture = reader.read();
    if (picture.isNull())
        return;
    m_shapes.insert(key, { QStringLiteral("data:image/png;base64,") + png, picture, hotspot, size.boundedTo(QSizeF(largestPointer, largestPointer)) });
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
    if (m_pointerAt.isEmpty() || key == m_cursorShape || found == m_shapes.cend())
        return;
    m_cursorShape = key;
    const auto cursor = cursorPicture(found->size, found->hotspot, window() ? window()->effectiveDevicePixelRatio() : 1.0);
    auto pixmap = QPixmap::fromImage(found->picture.scaled(cursor.pixels, Qt::IgnoreAspectRatio, Qt::SmoothTransformation));
    pixmap.setDevicePixelRatio(cursor.scale);
    setCursor(QCursor(pixmap, cursor.hotspot.x(), cursor.hotspot.y()));
}

void ScreenVideo::forget()
{
    m_displays.clear();
    m_windows.clear();
    m_source.clear();
    m_access.clear();
    m_away.clear();
    m_control = true;
    m_claimed.invalidate();
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

// The screen's sources, how it shows and where this view looked stay with the terminal, so the
// view comes back as it was.
QVariantMap ScreenVideo::kept() const
{
    return { { QStringLiteral("displays"), m_displays }, { QStringLiteral("windows"), m_windows },
        { QStringLiteral("source"), m_source }, { QStringLiteral("access"), m_access }, { QStringLiteral("away"), m_away },
        { QStringLiteral("size"), m_sourceSize }, { QStringLiteral("zoom"), m_zoom }, { QStringLiteral("center"), m_center } };
}

void ScreenVideo::resume(const QVariantMap& kept)
{
    m_displays = kept.value(QStringLiteral("displays")).toList();
    m_windows = kept.value(QStringLiteral("windows")).toList();
    m_source = kept.value(QStringLiteral("source")).toMap();
    m_access = kept.value(QStringLiteral("access")).toMap();
    m_away = kept.value(QStringLiteral("away")).toString();
    m_sourceSize = kept.value(QStringLiteral("size")).toSizeF();
    m_zoom = kept.value(QStringLiteral("zoom"), 1.0).toReal();
    m_center = kept.value(QStringLiteral("center")).toPointF();
    emit sourcesChanged();
    emit sourceChanged();
    emit accessChanged();
    emit computerPointerChanged();
    emit placed();
    update();
}

void ScreenVideo::setShared(bool shared)
{
    if (shared == m_shared)
        return;
    m_shared = shared;
    if (shared)
        join();
    else
        pause();
    restate();
}

// Without Accessibility, Kodosi Screen cannot play input, and the view still zooms.
bool ScreenVideo::takesInput() const
{
    return !m_access.contains(QStringLiteral("input")) || m_access.value(QStringLiteral("input")).toBool();
}

QString ScreenVideo::away() const
{
    if (const auto offline = StreamVideo::away(); !offline.isEmpty())
        return offline;
    if (!m_shared)
        return QStringLiteral("sharingOff");
    if (!m_away.isEmpty())
        return m_away;
    if (m_access.contains(QStringLiteral("screen")) && !m_access.value(QStringLiteral("screen")).toBool())
        return QStringLiteral("recording");
    return {};
}

// A new pipe is a new seat at the screen, which shows the whole source.
void ScreenVideo::seated()
{
    m_sentLook.reset();
    m_claimed.invalidate();
}

void ScreenVideo::settled()
{
    emit placed();
    emit computerPointerChanged();
    sendLook();
}

void ScreenVideo::moved()
{
    if (m_control)
        return;
    emit placed();
    emit computerPointerChanged();
}

void ScreenVideo::acting()
{
    takeControl();
    sendLook();
}

qreal ScreenVideo::fit() const
{
    if (m_sourceSize.isEmpty() || width() <= 0 || height() <= 0)
        return 0;
    return std::min(width() / m_sourceSize.width(), height() / m_sourceSize.height());
}

// The part of the source that the device driving the capture shows, which the newest picture
// carries, or this view's own.
std::optional<QRectF> ScreenVideo::look() const
{
    if (m_control)
        return ownLook();
    if (fit() <= 0)
        return std::nullopt;
    return shownRegion().value_or(QRectF(QPointF(), m_sourceSize));
}

std::optional<QRectF> ScreenVideo::ownLook() const
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

// The part of the source that the newest picture shows, when the computer told one that a view can
// place.
std::optional<QRectF> ScreenVideo::shownRegion() const
{
    const auto region = pictureRegion();
    if (!region || !std::isfinite(region->x()) || !std::isfinite(region->y()) || measured(region->width(), region->height()).isEmpty())
        return std::nullopt;
    return region;
}

// The picture of a part of the source shows where that part is, so a zoom moves and scales the
// newest picture at once, before the screen sends the part that the view shows.
QRectF ScreenVideo::pictureRect() const
{
    const auto shown = look();
    if (!shown)
        return StreamVideo::pictureRect();
    const auto region = shownRegion().value_or(QRectF(QPointF(), m_sourceSize));
    const auto scale = std::min(width() / shown->width(), height() / shown->height());
    return { viewPoint(region.topLeft(), *shown), region.size() * scale };
}

QVariantMap ScreenVideo::computerPointer() const
{
    const auto shown = look();
    const auto shape = m_shapes.constFind(shapeKey(m_pointerAt.value(QStringLiteral("shape"))));
    const QPointF pointed(m_pointerAt.value(QStringLiteral("x")).toDouble(), m_pointerAt.value(QStringLiteral("y")).toDouble());
    if (!shown || m_pointerAt.isEmpty() || shape == m_shapes.cend() || !std::isfinite(pointed.x()) || !std::isfinite(pointed.y()))
        return {};
    const auto scale = std::max(std::min(width() / shown->width(), height() / shown->height()), smallestPointer);
    const auto at = viewPoint(pointed, *shown);
    return { { QStringLiteral("x"), at.x() - shape->hotspot.x() * scale }, { QStringLiteral("y"), at.y() - shape->hotspot.y() * scale },
        { QStringLiteral("width"), shape->size.width() * scale }, { QStringLiteral("height"), shape->size.height() * scale },
        { QStringLiteral("image"), shape->address } };
}

// Acting here takes the capture over, so this view shows its own look at once.
void ScreenVideo::takeControl()
{
    m_claimed.start();
    if (m_control)
        return;
    m_control = true;
    emit placed();
    emit computerPointerChanged();
    update();
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
    takeControl();
    m_zoom = next;
    const auto zoomed = whole * next;
    m_center = QPointF(held->x() - (x - width() / 2) / zoomed, held->y() - (y - height() / 2) / zoomed);
    looked();
}

bool ScreenVideo::pan(qreal dx, qreal dy)
{
    const auto whole = fit();
    const auto before = ownLook();
    if (m_zoom <= 1 || !before)
        return false;
    m_center += QPointF(dx, dy) / (whole * m_zoom);
    if (ownLook() == before) {
        m_center = before->center();
        return false;
    }
    takeControl();
    looked();
    return true;
}

void ScreenVideo::showWhole()
{
    if (m_zoom <= 1)
        return;
    takeControl();
    m_zoom = 1;
    looked();
}

void ScreenVideo::show(const QString& kind, qint64 id)
{
    post({ { QStringLiteral("type"), QStringLiteral("show") },
        { QStringLiteral("source"), QJsonObject { { QStringLiteral("kind"), kind }, { QStringLiteral("id"), id } } } });
}

// Text only, and none past what the screen takes, so a paste there pastes what it had then.
void ScreenVideo::giveClipboard()
{
    const auto text = QGuiApplication::clipboard()->text();
    if (text.isEmpty() || !takesInput() || text.toUtf8().size() > mostClipboardBytes)
        return;
    post({ { QStringLiteral("type"), QStringLiteral("clipboard") }, { QStringLiteral("text"), text } });
}

// Keeps the middle where the source is, so that a pan back starts at once.
void ScreenVideo::looked()
{
    if (const auto shown = ownLook())
        m_center = shown->center();
    emit placed();
    emit computerPointerChanged();
    update();
    sendLook();
}

// Sends what the view shows at most every `lookEvery`, and the last of a zoom or pan once it ends.
// Each look names its source, so the screen leaves one that comes after another source took its
// place. A look takes the capture over, so a view that follows another sends none.
void ScreenVideo::sendLook()
{
    if (!watching() || m_source.isEmpty() || !m_control)
        return;
    const auto shown = m_zoom > 1 ? ownLook() : std::nullopt;
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
