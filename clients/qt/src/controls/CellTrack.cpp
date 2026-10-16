#include "controls/CellTrack.hpp"

#include <QPointF>
#include <QSGNode>
#include <QtQuick/private/qquickitem_p.h>
#include <QtQuick/private/qsgadaptationlayer_p.h>
#include <QtQuick/private/qsgcontext_p.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <numbers>
#include <variant>

namespace kodosi {
namespace cells {
namespace {

constexpr std::int64_t strokesPerRound = 64;
constexpr std::array<double, 4> coolingSteps { 0.14, 0.45, 0.9, 1.4 };

struct Rhythm {
    std::array<double, strokesPerRound> offsets {};
    double period = 0;
};

Rhythm makeRhythm()
{
    std::uint32_t state = 0x2545f491;
    const auto random = [&state] {
        state += 0x6d2b79f5;
        std::uint32_t value = (state ^ (state >> 15)) * (1 | state);
        value = (value + (value ^ (value >> 7)) * (61 | value)) ^ value;
        return static_cast<double>(value ^ (value >> 14)) / 4294967296.0;
    };
    Rhythm rhythm;
    int burst = 0;
    double sum = 0;
    for (auto& offset : rhythm.offsets) {
        if (burst == 0) {
            burst = 5 + static_cast<int>(random() * 9);
            sum += 0.3 + random() * 0.45;
        } else {
            sum += 0.045 + random() * 0.07;
        }
        burst -= 1;
        offset = sum;
    }
    rhythm.period = rhythm.offsets.back();
    return rhythm;
}

const Rhythm& rhythm()
{
    static const Rhythm value = makeRhythm();
    return value;
}

std::int64_t wrapped(std::int64_t value, std::int64_t count)
{
    return ((value % count) + count) % count;
}

struct Spot {
    double x = 0;
    double y = 0;
    double angle = 0;
};

struct Line {
    QPointF from;
    QPointF to;
};

struct Arc {
    QPointF center;
    double start = 0;
};

class Track {
public:
    Track(QSizeF size, double radius, double lineWidth)
        : m_left(lineWidth / 2)
        , m_top(lineWidth / 2)
        , m_width(std::max(0.0, size.width() - lineWidth))
        , m_height(std::max(0.0, size.height() - lineWidth))
        , m_corner(std::min(std::max(radius - lineWidth / 2, 0.0), std::min(m_width, m_height) / 2))
        , m_length(std::max(0.0, 2 * (m_width - 2 * m_corner) + 2 * (m_height - 2 * m_corner) + 2 * std::numbers::pi * m_corner))
    {
    }

    [[nodiscard]] double length() const { return m_length; }

    // The point at `at` of the way round, clockwise from the middle of the top edge, and
    // the direction of travel there.
    [[nodiscard]] Spot place(double at) const
    {
        const double right = m_left + m_width;
        const double bottom = m_top + m_height;
        const double middle = m_left + m_width / 2;
        const double r = m_corner;
        const std::array<std::variant<Line, Arc>, 9> pieces { {
            Line { { middle, m_top }, { right - r, m_top } },
            Arc { { right - r, m_top + r }, -std::numbers::pi / 2 },
            Line { { right, m_top + r }, { right, bottom - r } },
            Arc { { right - r, bottom - r }, 0 },
            Line { { right - r, bottom }, { m_left + r, bottom } },
            Arc { { m_left + r, bottom - r }, std::numbers::pi / 2 },
            Line { { m_left, bottom - r }, { m_left, m_top + r } },
            Arc { { m_left + r, m_top + r }, std::numbers::pi },
            Line { { m_left + r, m_top }, { middle, m_top } },
        } };
        double rest = (at - std::floor(at)) * m_length;
        for (const auto& piece : pieces) {
            if (const auto* line = std::get_if<Line>(&piece)) {
                const QPointF delta = line->to - line->from;
                const double span = std::hypot(delta.x(), delta.y());
                if (rest <= span && span > 0)
                    return { line->from.x() + delta.x() * rest / span, line->from.y() + delta.y() * rest / span,
                        std::atan2(delta.y(), delta.x()) };
                rest -= span;
            } else {
                const auto& arc = std::get<Arc>(piece);
                const double span = std::numbers::pi / 2 * r;
                if (rest <= span && span > 0) {
                    const double angle = arc.start + rest / r;
                    return { arc.center.x() + std::cos(angle) * r, arc.center.y() + std::sin(angle) * r,
                        angle + std::numbers::pi / 2 };
                }
                rest -= span;
            }
        }
        return { middle, m_top, 0 };
    }

private:
    double m_left;
    double m_top;
    double m_width;
    double m_height;
    double m_corner;
    double m_length;
};

QRectF centered(const Spot& spot, QSizeF size)
{
    return { std::floor(spot.x - size.width() / 2 + 0.5), std::floor(spot.y - size.height() / 2 + 0.5), size.width(),
        size.height() };
}

}

Ink Ink::cooling(double age)
{
    if (age < coolingSteps[0])
        return { .strong = true, .mix = 0 };
    if (age < coolingSteps[1])
        return { .mix = 0 };
    if (age < coolingSteps[2])
        return { .mix = 0.45 };
    if (age < coolingSteps[3])
        return { .mix = 0.72 };
    return {};
}

Ink Ink::filling(double reach)
{
    return { .mix = reach >= 1 ? 0 : reach <= 0 ? 1 : 1 - reach };
}

std::int64_t strokeAt(double seconds)
{
    const auto& value = rhythm();
    const auto round = static_cast<std::int64_t>(std::floor(seconds / value.period));
    const double within = seconds - static_cast<double>(round) * value.period;
    const auto index = std::upper_bound(value.offsets.begin(), value.offsets.end(), within) - value.offsets.begin();
    return round * strokesPerRound + index - 1;
}

double strokeTime(std::int64_t stroke)
{
    const auto& value = rhythm();
    const auto within = wrapped(stroke, strokesPerRound);
    return static_cast<double>((stroke - within) / strokesPerRound) * value.period + value.offsets[within];
}

Shape rim(QSizeF size, double radius, double lineWidth)
{
    Shape shape;
    const Track track(size, radius, lineWidth);
    if (track.length() <= 0 || lineWidth <= 0)
        return shape;
    const int count = std::max(8, static_cast<int>(track.length() / (lineWidth * 2.5)));
    const double cellLength = track.length() / count * 0.6;
    const double side = (cellLength + lineWidth) / 2;
    const QSizeF cursor(std::max(lineWidth * 1.5, cellLength), lineWidth * 2.6);
    for (int index = 0; index < count; ++index) {
        const auto spot = track.place((index + 0.5) / count);
        const double across = std::abs(std::cos(spot.angle));
        const double along = std::abs(std::sin(spot.angle));
        const QSizeF cell = std::abs(across - along) < 0.4 ? QSizeF(side, side)
            : across >= along                             ? QSizeF(cellLength, lineWidth)
                                                          : QSizeF(lineWidth, cellLength);
        shape.cells.push_back(centered(spot, cell));
        shape.cursors.push_back(centered(spot, cursor));
    }
    shape.cellRadius = lineWidth * 0.3;
    shape.cursorRadius = lineWidth * 0.35;
    return shape;
}

Frame typing(int count, double seconds)
{
    Frame frame;
    if (count <= 0)
        return frame;
    const auto head = strokeAt(seconds);
    for (int cell = 0; cell < count; ++cell)
        frame.inks.push_back(Ink::cooling(seconds - strokeTime(head - wrapped(head - cell, count))));
    const double idle = seconds - strokeTime(head);
    if (idle < 0.24 || static_cast<std::int64_t>(std::floor((idle - 0.24) / 0.42)) % 2 == 1)
        frame.cursor = static_cast<int>(wrapped(head, count));
    return frame;
}

Frame resting(int count)
{
    Frame frame;
    if (count <= 0)
        return frame;
    for (int cell = 0; cell < count; ++cell) {
        const auto ago = wrapped(-cell, count);
        frame.inks.push_back(Ink::cooling(ago < 4 ? static_cast<double>(ago) * 0.25 : 9));
    }
    frame.cursor = 0;
    return frame;
}

Frame progress(int count, double fraction, bool cursorLit)
{
    Frame frame;
    if (count <= 0)
        return frame;
    for (int cell = 0; cell < count; ++cell)
        frame.inks.push_back(Ink::filling(fraction * count - cell));
    if (fraction < 1 && cursorLit)
        frame.cursor = std::clamp(static_cast<int>(std::floor(fraction * count)), 0, count - 1);
    return frame;
}

double nextTypingChange(int count, double seconds)
{
    const auto head = strokeAt(seconds);
    double next = strokeTime(head + 1);
    for (auto stroke = head; stroke > head - count; --stroke) {
        const double struck = strokeTime(stroke);
        if (seconds - struck >= coolingSteps.back())
            break;
        for (const double step : coolingSteps) {
            if (struck + step > seconds) {
                next = std::min(next, struck + step);
                break;
            }
        }
    }
    const double blink = strokeTime(head) + 0.24;
    return std::min(next, seconds < blink ? blink : blink + (std::floor((seconds - blink) / 0.42) + 1) * 0.42);
}

}

namespace {

double clock()
{
    return std::chrono::duration<double>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

bool blinkLit(double seconds)
{
    return static_cast<std::int64_t>(std::floor(seconds / 0.53)) % 2 == 0;
}

QSGInternalRectangleNode* rectangle(QSGContext* context, const QRectF& rect, double radius)
{
    auto* node = context->createInternalRectangleNode();
    node->setRect(rect);
    node->setPenWidth(0);
    node->setRadius(radius);
    node->resetTopLeftRadius();
    node->resetTopRightRadius();
    node->resetBottomLeftRadius();
    node->resetBottomRightRadius();
    node->setAntialiasing(radius > 0);
    node->setGradientStops({});
    node->setGradientVertical(true);
    return node;
}

}

CellTrack::CellTrack(QQuickItem* parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
    m_timer.setSingleShot(true);
    m_timer.setTimerType(Qt::PreciseTimer);
    connect(&m_timer, &QTimer::timeout, this, &CellTrack::refresh);
    connect(this, &CellTrack::modeChanged, this, &CellTrack::refresh);
    connect(this, &CellTrack::fractionChanged, this, &CellTrack::refresh);
    connect(this, &CellTrack::stillChanged, this, &CellTrack::refresh);
    connect(this, &CellTrack::pausedChanged, this, &CellTrack::refresh);
    connect(this, &CellTrack::radiusChanged, this, &CellTrack::reshape);
    connect(this, &CellTrack::lineWidthChanged, this, &CellTrack::reshape);
    connect(this, &CellTrack::colorsChanged, this, [this] {
        m_inksChanged = true;
        update();
    });
}

void CellTrack::geometryChange(const QRectF& newGeometry, const QRectF& oldGeometry)
{
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size())
        reshape();
}

void CellTrack::itemChange(ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemVisibleHasChanged || change == ItemSceneChange)
        refresh();
}

void CellTrack::reshape()
{
    auto shape = cells::rim(size(), m_radius, m_lineWidth);
    if (shape.cells == m_shape.cells && shape.cursors == m_shape.cursors && shape.cellRadius == m_shape.cellRadius
        && shape.cursorRadius == m_shape.cursorRadius)
        return;
    m_shape = std::move(shape);
    m_shapeChanged = true;
    update();
    refresh();
}

void CellTrack::refresh()
{
    const int count = static_cast<int>(m_shape.cells.size());
    const double now = clock();
    const bool measured = m_mode == QLatin1StringView("progress");
    auto frame = measured ? cells::progress(count, m_fraction, m_still || blinkLit(now))
        : m_still         ? cells::resting(count)
                          : cells::typing(count, now);
    if (frame != m_frame) {
        m_frame = std::move(frame);
        m_inksChanged = true;
        update();
    }
    if (count == 0 || m_still || m_paused || !isVisible() || window() == nullptr || (measured && m_fraction >= 1)) {
        m_timer.stop();
        return;
    }
    // Every rim wakes on the same grid of 30 a second, so the rims of a window change together.
    const double next = measured ? (std::floor(now / 0.53) + 1) * 0.53
                                 : std::ceil(cells::nextTypingChange(count, now) * 30) / 30;
    m_timer.start(std::chrono::milliseconds(static_cast<int>(std::ceil((next - now) * 1000)) + 1));
}

QColor CellTrack::color(const cells::Ink& ink) const
{
    if (ink.strong)
        return m_strong;
    if (ink.mix <= 0)
        return m_accent;
    if (ink.mix >= 1)
        return m_soft;
    const auto blend = [&ink](float from, float to) {
        return static_cast<float>(from + (static_cast<double>(to) - from) * ink.mix);
    };
    return QColor::fromRgbF(blend(m_accent.redF(), m_soft.redF()), blend(m_accent.greenF(), m_soft.greenF()),
        blend(m_accent.blueF(), m_soft.blueF()), blend(m_accent.alphaF(), m_soft.alphaF()));
}

QSGNode* CellTrack::updatePaintNode(QSGNode* oldNode, UpdatePaintNodeData*)
{
    const auto count = m_shape.cells.size();
    if (count == 0 || m_frame.inks.size() != count) {
        delete oldNode;
        m_shapeChanged = true;
        return nullptr;
    }
    auto* root = oldNode;
    if (root == nullptr || m_shapeChanged) {
        delete root;
        auto* context = QQuickItemPrivate::get(this)->sceneGraphContext();
        root = new QSGNode;
        for (const auto& cell : m_shape.cells)
            root->appendChildNode(rectangle(context, cell, m_shape.cellRadius));
        root->appendChildNode(rectangle(context, m_shape.cursors.front(), m_shape.cursorRadius));
        m_shapeChanged = false;
        m_inksChanged = true;
    }
    if (m_inksChanged) {
        auto* node = root->firstChild();
        for (const auto& ink : m_frame.inks) {
            auto* cell = static_cast<QSGInternalRectangleNode*>(node);
            cell->setColor(color(ink));
            cell->update();
            node = node->nextSibling();
        }
        // A hidden cursor turns clear rather than leaving the tree, which would rebuild every batch of the window.
        auto* cursor = static_cast<QSGInternalRectangleNode*>(node);
        if (m_frame.cursor >= 0)
            cursor->setRect(m_shape.cursors[static_cast<std::size_t>(m_frame.cursor)]);
        cursor->setColor(m_frame.cursor >= 0 ? m_strong : QColor(Qt::transparent));
        cursor->update();
        m_inksChanged = false;
    }
    return root;
}

}
