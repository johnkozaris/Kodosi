#include "ui/SoftShadow.hpp"

#include <QHash>
#include <QHashFunctions>
#include <QMutex>
#include <QMutexLocker>
#include <QPainter>
#include <QQuickWindow>
#include <QSGImageNode>
#include <QSGNode>
#include <QSGTexture>

#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <vector>

namespace kodosi {
namespace {

// A wide blur needs few image pixels: the GPU smooths it as it scales up.
constexpr qreal largestImageBlur = 8;
constexpr qsizetype cachedImageLimit = 256;

struct ShadowKey {
    qreal width = 0;
    qreal height = 0;
    qreal radius = 0;
    qreal blur = 0;
    qreal scale = 0;
    QRgb color = 0;

    bool operator==(const ShadowKey&) const = default;
};

size_t qHash(const ShadowKey& key, const size_t seed = 0) noexcept
{
    return qHashMulti(seed, key.width, key.height, key.radius, key.blur, key.scale, key.color);
}

QImage shadowImage(const ShadowKey& key)
{
    static QMutex mutex;
    static QHash<ShadowKey, QImage> images;
    {
        const QMutexLocker lock(&mutex);
        if (const auto found = images.constFind(key); found != images.constEnd())
            return *found;
    }
    auto image = SoftShadow::render(
        QSizeF(key.width, key.height), key.radius, key.blur, key.scale, QColor::fromRgba(key.color));
    const QMutexLocker lock(&mutex);
    if (images.size() >= cachedImageLimit)
        images.clear();
    images.insert(key, image);
    return image;
}

std::vector<float> gaussian(const qreal sigma)
{
    const auto reach = static_cast<int>(std::ceil(sigma * 3));
    std::vector<float> weights(static_cast<size_t>(reach) * 2 + 1);
    float total = 0;
    for (int offset = -reach; offset <= reach; ++offset) {
        const auto weight = static_cast<float>(std::exp(-(offset * offset) / (2 * sigma * sigma)));
        weights[static_cast<size_t>(offset + reach)] = weight;
        total += weight;
    }
    for (auto& weight : weights)
        weight /= total;
    return weights;
}

// One pass of a separable blur: `count` lines of `length` values, `step` apart
// along a line and `stride` apart between lines.
void blurPass(std::vector<float>& values, const std::vector<float>& weights, const int count, const int length,
    const int step, const int stride)
{
    const auto reach = static_cast<int>(weights.size() / 2);
    std::vector<float> line(static_cast<size_t>(length));
    for (int index = 0; index < count; ++index) {
        const auto start = static_cast<size_t>(index) * static_cast<size_t>(stride);
        for (int position = 0; position < length; ++position)
            line[static_cast<size_t>(position)] = values[start + static_cast<size_t>(position * step)];
        for (int position = 0; position < length; ++position) {
            float sum = 0;
            const auto first = std::max(-reach, -position);
            const auto last = std::min(reach, length - 1 - position);
            for (int offset = first; offset <= last; ++offset)
                sum += weights[static_cast<size_t>(offset + reach)] * line[static_cast<size_t>(position + offset)];
            values[start + static_cast<size_t>(position * step)] = sum;
        }
    }
}

struct Span {
    qreal sourceStart = 0;
    qreal sourceEnd = 0;
    qreal targetStart = 0;
    qreal targetEnd = 0;
};

// The image either matches the item along an axis, or holds a corner, one
// uniform pixel to stretch, and the other corner.
std::vector<Span> spans(const qreal length, const bool stretches, const qreal reach, const qreal border,
    const qreal scale)
{
    const auto start = -reach;
    const auto end = length + reach;
    if (!stretches)
        return { Span { 0, (end - start) * scale, start, end } };
    const auto imageLength = 2 * border + 1;
    return {
        Span { 0, border * scale, start, start + border },
        Span { border * scale, (border + 1) * scale, start + border, end - border },
        Span { (border + 1) * scale, imageLength * scale, end - border, end },
    };
}

class ShadowNode final : public QSGNode {
public:
    std::unique_ptr<QSGTexture> texture;
    ShadowKey key;
    std::array<QSGImageNode*, 9> patches {};
};

}

SoftShadow::SoftShadow(QQuickItem* parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
}

qreal SoftShadow::radius() const noexcept
{
    return m_radius;
}

void SoftShadow::setRadius(const qreal radius)
{
    if (m_radius == radius)
        return;
    m_radius = radius;
    emit radiusChanged();
    update();
}

qreal SoftShadow::blur() const noexcept
{
    return m_blur;
}

void SoftShadow::setBlur(const qreal blur)
{
    if (m_blur == blur)
        return;
    m_blur = blur;
    emit blurChanged();
    update();
}

QColor SoftShadow::color() const
{
    return m_color;
}

void SoftShadow::setColor(const QColor& color)
{
    if (m_color == color)
        return;
    m_color = color;
    emit colorChanged();
    update();
}

qreal SoftShadow::reach(const qreal blur) noexcept
{
    return blur > 0 ? std::ceil(blur * 3) : 0;
}

QImage SoftShadow::render(const QSizeF shape, const qreal radius, const qreal blur, const qreal scale,
    const QColor& color)
{
    const auto pad = reach(blur);
    const auto width = std::max(1, static_cast<int>(std::ceil((shape.width() + 2 * pad) * scale)));
    const auto height = std::max(1, static_cast<int>(std::ceil((shape.height() + 2 * pad) * scale)));
    QImage mask(width, height, QImage::Format_ARGB32_Premultiplied);
    mask.fill(Qt::transparent);
    {
        QPainter painter(&mask);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setPen(Qt::NoPen);
        painter.setBrush(Qt::black);
        painter.scale(scale, scale);
        painter.drawRoundedRect(QRectF(QPointF(pad, pad), shape), radius, radius);
    }
    std::vector<float> alpha(static_cast<size_t>(width) * static_cast<size_t>(height));
    for (int y = 0; y < height; ++y) {
        const auto* line = reinterpret_cast<const QRgb*>(mask.constScanLine(y));
        for (int x = 0; x < width; ++x)
            alpha[static_cast<size_t>(y) * static_cast<size_t>(width) + static_cast<size_t>(x)]
                = static_cast<float>(qAlpha(line[x])) / 255.0F;
    }
    if (blur > 0) {
        const auto weights = gaussian(blur * scale);
        blurPass(alpha, weights, height, width, 1, width);
        blurPass(alpha, weights, width, height, width, 1);
    }
    QImage image(width, height, QImage::Format_ARGB32_Premultiplied);
    const auto strength = static_cast<float>(color.alphaF());
    for (int y = 0; y < height; ++y) {
        auto* line = reinterpret_cast<QRgb*>(image.scanLine(y));
        for (int x = 0; x < width; ++x) {
            const auto value
                = alpha[static_cast<size_t>(y) * static_cast<size_t>(width) + static_cast<size_t>(x)] * strength;
            const auto level = std::clamp(static_cast<int>(std::lround(value * 255.0F)), 0, 255);
            line[x] = qPremultiply(qRgba(color.red(), color.green(), color.blue(), level));
        }
    }
    return image;
}

QSGNode* SoftShadow::updatePaintNode(QSGNode* node, UpdatePaintNodeData*)
{
    const auto bounds = size();
    if (m_blur <= 0 || bounds.width() <= 0 || bounds.height() <= 0 || m_color.alpha() == 0 || !window()) {
        delete node;
        return nullptr;
    }
    auto* shadow = static_cast<ShadowNode*>(node);
    if (!shadow)
        shadow = new ShadowNode;

    const auto scale = std::min(window()->effectiveDevicePixelRatio(), largestImageBlur / m_blur);
    const auto radius = std::clamp(m_radius, 0.0, std::min(bounds.width(), bounds.height()) / 2);
    const auto pad = reach(m_blur);
    // The corner and its blur end this far from the image edge.
    const auto border = pad + std::ceil(radius + 3 * m_blur);
    const auto stretchesX = bounds.width() + 2 * pad >= 2 * border + 1;
    const auto stretchesY = bounds.height() + 2 * pad >= 2 * border + 1;
    const ShadowKey key {
        .width = stretchesX ? 2 * border + 1 - 2 * pad : bounds.width(),
        .height = stretchesY ? 2 * border + 1 - 2 * pad : bounds.height(),
        .radius = radius,
        .blur = m_blur,
        .scale = scale,
        .color = m_color.rgba(),
    };
    std::unique_ptr<QSGTexture> previous;
    if (!shadow->texture || !(shadow->key == key)) {
        previous = std::move(shadow->texture);
        shadow->texture.reset(window()->createTextureFromImage(shadowImage(key), QQuickWindow::TextureCanUseAtlas));
        shadow->key = key;
    }

    const auto columns = spans(bounds.width(), stretchesX, pad, border, scale);
    const auto rows = spans(bounds.height(), stretchesY, pad, border, scale);
    for (size_t index = 0; index < shadow->patches.size(); ++index) {
        const auto column = index % 3;
        const auto row = index / 3;
        auto*& patch = shadow->patches[index];
        if (column >= columns.size() || row >= rows.size()) {
            if (patch) {
                shadow->removeChildNode(patch);
                delete patch;
                patch = nullptr;
            }
            continue;
        }
        const auto& x = columns[column];
        const auto& y = rows[row];
        const auto created = patch == nullptr;
        if (created) {
            patch = window()->createImageNode();
            patch->setFiltering(QSGTexture::Linear);
        }
        patch->setTexture(shadow->texture.get());
        patch->setSourceRect(QRectF(QPointF(x.sourceStart, y.sourceStart), QPointF(x.sourceEnd, y.sourceEnd)));
        patch->setRect(QRectF(QPointF(x.targetStart, y.targetStart), QPointF(x.targetEnd, y.targetEnd)));
        if (created)
            shadow->appendChildNode(patch);
    }
    return shadow;
}

void SoftShadow::geometryChange(const QRectF& next, const QRectF& previous)
{
    QQuickItem::geometryChange(next, previous);
    if (next.size() != previous.size())
        update();
}

void SoftShadow::itemChange(const ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged || change == ItemSceneChange)
        update();
}

}
