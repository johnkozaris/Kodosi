#include "controls/LightTrack.hpp"

#include <QCoreApplication>
#include <QDateTime>
#include <QPointer>
#include <QQuickWindow>
#include <QRectF>
#include <QSGGeometryNode>
#include <QSGVertexColorMaterial>
#include <QTimer>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <numbers>
#include <utility>

namespace kodosi {

namespace light {
namespace {

constexpr std::size_t faceRays = 60;
constexpr std::size_t rings = 8;
constexpr std::size_t stops = 36;
constexpr double track = 0.2;
constexpr double meterTrack = 0.22;
constexpr double spill = 0.34;
constexpr double reach = 0.42;
constexpr std::array<double, 3> core { 1, 0.96, 0.92 };

struct Sample {
    QPointF point;
    QPointF normal;
};

struct Rgba {
    float red = 0;
    float green = 0;
    float blue = 0;
    float alpha = 0;
};

QPointF direction(double degrees)
{
    const double radians = degrees * std::numbers::pi / 180;
    return { std::sin(radians), -std::cos(radians) };
}

double clockwise(QPointF from, QPointF to)
{
    const double degrees = std::atan2(to.x() - from.x(), from.y() - to.y()) * 180 / std::numbers::pi;
    return degrees < 0 ? degrees + 360 : degrees;
}

QPointF unit(QPointF vector)
{
    const double length = std::hypot(vector.x(), vector.y());
    return length > 0 ? vector / length : QPointF();
}

QPointF outward(QPointF tangent)
{
    return unit(QPointF(tangent.y(), -tangent.x()));
}

std::vector<Sample> squircle(const QRectF& rect, double radius)
{
    static constexpr std::array<std::array<double, 2>, 12> curve { { { 1.52866483, 0 }, { 1.08849323, 0 },
        { 0.86840689, 0.02189748 }, { 0.66993427, 0.065496 }, { 0.63149399, 0.074911 }, { 0.37282392, 0.16905899 },
        { 0.16905899, 0.37282392 }, { 0.074911, 0.63149399 }, { 0.065496, 0.66993427 }, { 0.02189748, 0.86840689 },
        { 0, 1.08849323 }, { 0, 1.52866483 } } };
    constexpr int steps = 12;
    const double r = std::clamp(radius, 0.0, std::min(rect.width(), rect.height()) / 3.05732966);
    const auto corner = [&rect, r](int which, std::size_t index) {
        const double u = curve[index][0] * r;
        const double v = curve[index][1] * r;
        switch (which) {
        case 0:
            return QPointF(rect.right() - u, rect.top() + v);
        case 1:
            return QPointF(rect.right() - v, rect.bottom() - u);
        case 2:
            return QPointF(rect.left() + u, rect.bottom() - v);
        default:
            return QPointF(rect.left() + v, rect.top() + u);
        }
    };
    std::vector<Sample> samples;
    QPointF from = corner(3, 11);
    QPointF arriving = from - corner(3, 10);
    const auto line = [&](QPointF to) {
        if (to == from)
            return;
        samples.push_back({ from, unit(outward(arriving) + outward(to - from)) });
        arriving = to - from;
        from = to;
    };
    const auto cubic = [&](QPointF first, QPointF second, QPointF to) {
        if (first == from && second == from && to == from)
            return;
        for (int step = 0; step < steps; ++step) {
            const double t = static_cast<double>(step) / steps;
            const double s = 1 - t;
            const QPointF tangent = s * s * (first - from) + 2 * s * t * (second - first) + t * t * (to - second);
            samples.push_back({ s * s * s * from + 3 * s * s * t * first + 3 * s * t * t * second + t * t * t * to,
                step == 0 ? unit(outward(arriving) + outward(tangent)) : outward(tangent) });
        }
        arriving = to - second;
        from = to;
    };
    for (int which = 0; which < 4; ++which) {
        line(corner(which, 0));
        cubic(corner(which, 1), corner(which, 2), corner(which, 3));
        line(corner(which, 4));
        cubic(corner(which, 5), corner(which, 6), corner(which, 7));
        line(corner(which, 8));
        cubic(corner(which, 9), corner(which, 10), corner(which, 11));
    }
    return samples;
}

std::vector<Sample> capsule(const QRectF& rect)
{
    constexpr int steps = 30;
    const double r = std::min(rect.width(), rect.height()) / 2;
    const std::array centres { QPointF(rect.right() - r, rect.top() + r), QPointF(rect.right() - r, rect.bottom() - r),
        QPointF(rect.left() + r, rect.bottom() - r), QPointF(rect.left() + r, rect.top() + r) };
    std::vector<Sample> samples;
    for (std::size_t which = 0; which < centres.size(); ++which) {
        const double start = (static_cast<double>(which) - 1) * std::numbers::pi / 2;
        const QPointF facing(std::cos(start), std::sin(start));
        const QPointF edge = centres[(which + 3) % 4] + r * facing;
        if (edge != centres[which] + r * facing)
            samples.push_back({ edge, facing });
        for (int step = 0; step < steps; ++step) {
            const double angle = start + std::numbers::pi / 2 * step / steps;
            const QPointF normal(std::cos(angle), std::sin(angle));
            samples.push_back({ centres[which] + r * normal, normal });
        }
    }
    return samples;
}

Curve offset(const std::vector<Sample>& outline, double distance, QPointF centre)
{
    std::vector<std::pair<double, QPointF>> points;
    points.reserve(outline.size());
    for (const auto& sample : outline) {
        const QPointF point = sample.point + sample.normal * distance;
        points.emplace_back(clockwise(centre, point), point);
    }
    std::rotate(points.begin(),
        std::min_element(points.begin(), points.end(), [](const auto& a, const auto& b) { return a.first < b.first; }),
        points.end());
    Curve curve;
    for (const auto& [angle, point] : points) {
        if (!curve.angles.empty() && angle <= curve.angles.back())
            continue;
        curve.angles.push_back(angle);
        curve.points.push_back(point);
    }
    return curve;
}

QPointF cross(const Curve& curve, QPointF centre, double degrees)
{
    const auto count = curve.points.size();
    if (count < 2)
        return centre;
    double angle = std::fmod(degrees, 360.0);
    if (angle < 0)
        angle += 360;
    const auto after = static_cast<std::size_t>(
        std::upper_bound(curve.angles.begin(), curve.angles.end(), angle) - curve.angles.begin());
    const QPointF from = curve.points[(after + count - 1) % count];
    const QPointF along = curve.points[after % count] - from;
    const QPointF ray = direction(angle);
    const double turn = ray.x() * along.y() - ray.y() * along.x();
    if (std::abs(turn) < 1e-12)
        return from;
    const QPointF start = from - centre;
    return centre + ray * ((start.x() * along.y() - start.y() * along.x()) / turn);
}

const std::array<double, stops + 1>& glows()
{
    static const auto values = [] {
        std::array<double, stops + 1> result {};
        for (std::size_t stop = 0; stop <= stops; ++stop) {
            const double turn = 360.0 * static_cast<double>(stop) / stops;
            result[stop] = glow(stop * 2 <= stops ? turn : turn - 360);
        }
        return result;
    }();
    return values;
}

std::pair<std::size_t, double> between(double degrees)
{
    double turn = std::fmod(degrees, 360.0) / 360;
    if (turn < 0)
        turn += 1;
    const double position = turn * stops;
    const auto stop = std::min(static_cast<std::size_t>(position), stops - 1);
    return { stop, position - static_cast<double>(stop) };
}

Rgba tinted(const QColor& colour, double opacity)
{
    const double alpha = opacity * colour.alphaF();
    return { static_cast<float>(colour.redF() * alpha), static_cast<float>(colour.greenF() * alpha),
        static_cast<float>(colour.blueF() * alpha), static_cast<float>(alpha) };
}

Rgba blend(const Rgba& from, const Rgba& to, double amount)
{
    const auto channel = [amount](float a, float b) { return static_cast<float>(a + (b - a) * amount); };
    return { channel(from.red, to.red), channel(from.green, to.green), channel(from.blue, to.blue),
        channel(from.alpha, to.alpha) };
}

std::array<Rgba, stops + 1> warmStops(const QColor& light)
{
    std::array<Rgba, stops + 1> result;
    for (std::size_t stop = 0; stop <= stops; ++stop) {
        const double strength = glows()[stop];
        const double white = strength * strength * strength;
        const double alpha = strength * light.alphaF();
        const auto channel = [white, alpha](double from, double to) {
            return static_cast<float>((from + (to - from) * white) * alpha);
        };
        result[stop] = { channel(light.redF(), core[0]), channel(light.greenF(), core[1]),
            channel(light.blueF(), core[2]), static_cast<float>(alpha) };
    }
    return result;
}

QSGGeometry::ColoredPoint2D vertex(QPointF point, const Rgba& colour)
{
    const auto byte = [](float value) { return static_cast<uchar>(std::clamp(value, 0.0F, 1.0F) * 255 + 0.5F); };
    QSGGeometry::ColoredPoint2D result {};
    result.set(static_cast<float>(point.x()), static_cast<float>(point.y()), byte(colour.red), byte(colour.green),
        byte(colour.blue), byte(colour.alpha));
    return result;
}

void put(std::vector<QSGGeometry::ColoredPoint2D>& vertices, const Section& section, const Rgba& colour)
{
    vertices.push_back(vertex(section.points[0], {}));
    vertices.push_back(vertex(section.points[1], colour));
    vertices.push_back(vertex(section.points[2], colour));
    vertices.push_back(vertex(section.points[3], {}));
}

double coverage(double degrees, double end, double startSpan, double endSpan)
{
    if (end <= 0)
        return 0;
    if (end >= 360)
        return 1;
    const auto ramp = [](double inside, double span) { return std::clamp(0.5 + inside / (2 * span), 0.0, 1.0); };
    if (degrees <= end)
        return std::min(ramp(degrees, startSpan), ramp(end - degrees, endSpan));
    return std::max(ramp(degrees - 360, startSpan), ramp(end - degrees, endSpan));
}

}

double glow(double degrees)
{
    const double spread = degrees >= 0 ? 20 : 36;
    return std::exp(-degrees * degrees / (2 * spread * spread));
}

double phase(std::int64_t milliseconds)
{
    constexpr std::int64_t turn = 4000;
    return 360.0 * static_cast<double>(((milliseconds % turn) + turn) % turn) / turn;
}

Mesh::Mesh(const Shape& shape)
    : m_shape(shape)
    , m_centre(shape.size.width() / 2, shape.size.height() / 2)
{
    const double width = shape.lineWidth;
    const double fringe = shape.pixel / 2;
    const QRectF bounds(QPointF(), shape.size);
    const QRectF middle = bounds.adjusted(width / 2, width / 2, -width / 2, -width / 2);
    if (width <= 0 || fringe <= 0 || width / 2 + fringe >= std::min(middle.width(), middle.height()) / 2)
        return;
    const double solid = std::max(width / 2 - fringe, 0.0);
    const auto outline = shape.radius < 0 ? capsule(middle) : squircle(middle, shape.radius - width / 2);
    const std::array offsets { width / 2 + fringe, solid, -solid, -width / 2 - fringe };
    for (std::size_t edge = 0; edge < offsets.size(); ++edge)
        m_edges[edge] = offset(outline, offsets[edge], m_centre);
    m_ring.reserve(rays);
    for (int ray = 0; ray < rays; ++ray)
        m_ring.push_back(section(360.0 * ray / rays));
    if (!shape.face)
        return;
    const auto tile = shape.radius < 0 ? capsule(bounds) : squircle(bounds, shape.radius);
    const auto inside = offset(tile, -fringe, m_centre);
    const auto outside = offset(tile, fringe, m_centre);
    m_face.reserve(faceRays);
    for (std::size_t ray = 0; ray < faceRays; ++ray) {
        const double degrees = 360.0 * static_cast<double>(ray) / faceRays;
        m_face.push_back({ cross(inside, m_centre, degrees), cross(outside, m_centre, degrees) });
    }
}

bool Mesh::paint(const Look& look)
{
    const Layout layout = m_ring.empty() ? Layout::None
        : look.progress                  ? Layout::Progress
        : look.still                     ? Layout::Steady
        : m_shape.face                   ? Layout::Tile
                                         : Layout::Ring;
    m_vertices.clear();
    if (layout == Layout::Tile)
        warm(look);
    if (layout == Layout::Tile || layout == Layout::Ring)
        turn(look);
    if (layout == Layout::Progress)
        meter(look);
    if (layout == Layout::Steady)
        steady(look);
    if (layout == m_layout)
        return false;
    m_layout = layout;
    index();
    return true;
}

Section Mesh::section(double degrees) const
{
    Section result { degrees, {} };
    for (std::size_t edge = 0; edge < m_edges.size(); ++edge)
        result.points[edge] = cross(m_edges[edge], m_centre, degrees);
    return result;
}

double Mesh::pixelAngle(const Section& section) const
{
    const QPointF middle = (section.points[1] + section.points[2]) / 2 - m_centre;
    return std::atan2(m_shape.pixel / 2, std::hypot(middle.x(), middle.y())) * 180 / std::numbers::pi;
}

void Mesh::warm(const Look& look)
{
    const double side = std::min(m_shape.size.width(), m_shape.size.height());
    const QPointF source = m_centre + side / 2 * direction(look.angle);
    const double radius = reach * side;
    const auto warmed = [&](QPointF point) {
        const QPointF away = point - source;
        return vertex(point, tinted(look.light, spill * std::max(0.0, 1 - std::hypot(away.x(), away.y()) / radius)));
    };
    m_vertices.push_back(warmed(m_centre));
    for (const auto& [inside, outside] : m_face) {
        for (std::size_t ring = 1; ring <= rings; ++ring)
            m_vertices.push_back(warmed(m_centre + (inside - m_centre) * (static_cast<double>(ring) / rings)));
        m_vertices.push_back(vertex(outside, {}));
    }
}

void Mesh::turn(const Look& look)
{
    const auto warmth = m_shape.face ? warmStops(look.light) : std::array<Rgba, stops + 1> {};
    for (const auto& section : m_ring) {
        const auto [stop, amount] = between(section.angle - look.angle);
        const double strength = glows()[stop] + (glows()[stop + 1] - glows()[stop]) * amount;
        put(m_vertices, section,
            m_shape.face ? blend(warmth[stop], warmth[stop + 1], amount)
                         : tinted(look.light, track + (1 - track) * strength));
    }
}

void Mesh::meter(const Look& look)
{
    const double end = 360 * std::clamp(look.fraction, 0.0, 1.0);
    const double startSpan = pixelAngle(m_ring.front());
    const double endSpan = pixelAngle(section(end));
    std::array cuts { startSpan, 360 - startSpan, end - endSpan, end + endSpan };
    for (auto& cut : cuts)
        cut = std::clamp(cut, 0.0, 360.0);
    std::sort(cuts.begin(), cuts.end());
    const auto place = [&](const Section& section) {
        put(m_vertices, section,
            tinted(look.light, meterTrack + (1 - meterTrack) * coverage(section.angle, end, startSpan, endSpan)));
    };
    auto cut = cuts.begin();
    for (const auto& section : m_ring) {
        for (; cut != cuts.end() && *cut < section.angle; ++cut)
            place(this->section(*cut));
        place(section);
    }
    for (; cut != cuts.end(); ++cut)
        place(section(*cut));
    place(Section { 360, m_ring.front().points });
}

void Mesh::steady(const Look& look)
{
    const double warmth = m_shape.face ? 0.5 : 0;
    const auto warm = [warmth](double from, double to) { return static_cast<float>(from + (to - from) * warmth); };
    const QColor light = QColor::fromRgbF(warm(look.light.redF(), core[0]), warm(look.light.greenF(), core[1]),
        warm(look.light.blueF(), core[2]), look.light.alphaF());
    for (const auto& section : m_ring)
        put(m_vertices, section, tinted(light, 1));
}

void Mesh::index()
{
    m_indices.clear();
    const auto quad = [this](std::size_t a, std::size_t b, std::size_t c, std::size_t d) {
        for (const auto corner : { a, b, c, a, c, d })
            m_indices.push_back(static_cast<quint16>(corner));
    };
    std::size_t first = 0;
    if (m_layout == Layout::Tile) {
        const auto point = [](std::size_t ray, std::size_t ring) { return ray * (rings + 1) + ring; };
        for (std::size_t ray = 0; ray < faceRays; ++ray) {
            const std::size_t next = (ray + 1) % faceRays;
            for (const auto corner : { std::size_t { 0 }, point(ray, 1), point(next, 1) })
                m_indices.push_back(static_cast<quint16>(corner));
            for (std::size_t ring = 1; ring <= rings; ++ring)
                quad(point(ray, ring), point(next, ring), point(next, ring + 1), point(ray, ring + 1));
        }
        first = 1 + faceRays * (rings + 1);
    }
    if (m_vertices.size() <= first)
        return;
    const std::size_t sections = (m_vertices.size() - first) / 4;
    const std::size_t spans = m_layout == Layout::Progress ? sections - 1 : sections;
    for (std::size_t span = 0; span < spans; ++span) {
        const std::size_t here = first + 4 * span;
        const std::size_t there = first + 4 * ((span + 1) % sections);
        for (std::size_t band = 0; band < 3; ++band)
            quad(here + band, there + band, there + band + 1, here + band + 1);
    }
}

}

class LightClock final : public QObject {
public:
    static void join(LightTrack* light);
    static void leave(LightTrack* light);
    [[nodiscard]] static bool running();

private:
    LightClock();
    void tick();
    void schedule();

    QTimer m_timer;
    std::vector<LightTrack*> m_lights;
};

namespace {

QPointer<LightClock> sharedClock;

std::int64_t nextSharedTick(const std::int64_t now)
{
    constexpr std::int64_t ticksPerSecond = 30;
    return ((now * ticksPerSecond / 1000 + 1) * 1000 + ticksPerSecond - 1) / ticksPerSecond;
}

}

LightClock::LightClock()
    : QObject(QCoreApplication::instance())
{
    m_timer.setSingleShot(true);
    m_timer.setTimerType(Qt::PreciseTimer);
    connect(&m_timer, &QTimer::timeout, this, [this] { tick(); });
}

void LightClock::join(LightTrack* light)
{
    if (QCoreApplication::instance() == nullptr)
        return;
    if (sharedClock.isNull())
        sharedClock = new LightClock;
    auto& lights = sharedClock->m_lights;
    if (std::find(lights.begin(), lights.end(), light) == lights.end())
        lights.push_back(light);
    if (!sharedClock->m_timer.isActive())
        sharedClock->schedule();
}

void LightClock::leave(LightTrack* light)
{
    if (sharedClock.isNull())
        return;
    auto& lights = sharedClock->m_lights;
    lights.erase(std::remove(lights.begin(), lights.end(), light), lights.end());
    if (lights.empty())
        sharedClock->m_timer.stop();
}

bool LightClock::running()
{
    return !sharedClock.isNull() && sharedClock->m_timer.isActive();
}

void LightClock::tick()
{
    const double angle = light::phase(QDateTime::currentMSecsSinceEpoch());
    for (auto* light : m_lights)
        light->turnTo(angle);
    schedule();
}

void LightClock::schedule()
{
    if (m_lights.empty())
        return;
    const std::int64_t now = QDateTime::currentMSecsSinceEpoch();
    const std::int64_t next = nextSharedTick(now);
    m_timer.start(std::chrono::milliseconds(std::max<std::int64_t>(1, next - now)));
}

LightTrack::LightTrack(QQuickItem* parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
    for (const auto signal : { &LightTrack::fractionChanged, &LightTrack::radiusChanged, &LightTrack::lineWidthChanged,
             &LightTrack::tileChanged, &LightTrack::lightChanged })
        connect(this, signal, this, &QQuickItem::update);
    connect(this, &LightTrack::modeChanged, this, &LightTrack::refresh);
    connect(this, &LightTrack::stillChanged, this, &LightTrack::refresh);
}

LightTrack::~LightTrack()
{
    if (m_turning)
        LightClock::leave(this);
}

bool LightTrack::clockRunning()
{
    return LightClock::running();
}

void LightTrack::geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry)
{
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size())
        refresh();
}

void LightTrack::itemChange(ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemVisibleHasChanged || change == ItemSceneChange)
        refresh();
    else if (change == ItemDevicePixelRatioHasChanged)
        update();
}

void LightTrack::refresh()
{
    const bool turns = m_mode != QLatin1StringView("progress") && !m_still && isVisible() && window() != nullptr
        && width() > 0 && height() > 0;
    if (turns != m_turning) {
        m_turning = turns;
        if (turns)
            LightClock::join(this);
        else
            LightClock::leave(this);
    }
    if (turns)
        m_angle = light::phase(QDateTime::currentMSecsSinceEpoch());
    update();
}

void LightTrack::turnTo(double angle)
{
    if (angle == m_angle)
        return;
    m_angle = angle;
    update();
}

QSGNode* LightTrack::updatePaintNode(QSGNode* oldNode, UpdatePaintNodeData*)
{
    const light::Shape shape { size(), m_radius, m_lineWidth,
        window() != nullptr ? 1 / window()->effectiveDevicePixelRatio() : 1, m_tile };
    if (!(shape == m_mesh.shape()))
        m_mesh = light::Mesh(shape);
    const bool reindexed
        = m_mesh.paint({ m_mode == QLatin1StringView("progress"), m_fraction, m_angle, m_light, m_still });
    const auto& vertices = m_mesh.vertices();
    const auto& indices = m_mesh.indices();
    if (vertices.empty() || indices.empty()) {
        delete oldNode;
        return nullptr;
    }
    auto* node = static_cast<QSGGeometryNode*>(oldNode);
    if (node == nullptr) {
        node = new QSGGeometryNode;
        auto* geometry
            = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0, 0, QSGGeometry::UnsignedShortType);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        geometry->setVertexDataPattern(QSGGeometry::DynamicPattern);
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        node->setMaterial(new QSGVertexColorMaterial);
        node->setFlag(QSGNode::OwnsMaterial);
    }
    auto* geometry = node->geometry();
    const auto vertexCount = static_cast<int>(vertices.size());
    const auto indexCount = static_cast<int>(indices.size());
    const bool resized = geometry->vertexCount() != vertexCount || geometry->indexCount() != indexCount;
    if (resized)
        geometry->allocate(vertexCount, indexCount);
    if (resized || reindexed)
        std::copy(indices.begin(), indices.end(), geometry->indexDataAsUShort());
    std::copy(vertices.begin(), vertices.end(), geometry->vertexDataAsColoredPoint2D());
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

}
