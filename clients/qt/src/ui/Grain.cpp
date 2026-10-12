#include "ui/Grain.hpp"

#include <QQuickWindow>
#include <QSGImageNode>
#include <QSGNode>
#include <QSGTexture>

#include <algorithm>
#include <cmath>
#include <memory>
#include <vector>

namespace kodosi {
namespace {

class GrainNode final : public QSGNode {
public:
    std::unique_ptr<QSGTexture> texture;
    bool dark = false;
    std::vector<QSGImageNode*> tiles;
};

}

Grain::Grain(QQuickItem* parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
}

bool Grain::dark() const noexcept
{
    return m_dark;
}

void Grain::setDark(const bool dark)
{
    if (m_dark == dark)
        return;
    m_dark = dark;
    emit darkChanged();
    update();
}

QImage Grain::tile(const bool dark)
{
    QImage image(tileSize, tileSize, QImage::Format_ARGB32_Premultiplied);
    quint64 state = 0x9E3779B97F4A7C15ULL;
    for (int y = 0; y < tileSize; ++y) {
        auto* line = reinterpret_cast<QRgb*>(image.scanLine(y));
        for (int x = 0; x < tileSize; ++x) {
            state = state * 6364136223846793005ULL + 1442695040888963407ULL;
            const auto value = static_cast<int>(state >> 56);
            line[x] = dark ? qRgba(value, value, value, value) : qRgba(0, 0, 0, 255 - value);
        }
    }
    return image;
}

QSGNode* Grain::updatePaintNode(QSGNode* node, UpdatePaintNodeData*)
{
    const auto bounds = size();
    if (bounds.width() <= 0 || bounds.height() <= 0 || !window()) {
        delete node;
        return nullptr;
    }
    auto* grain = static_cast<GrainNode*>(node);
    if (!grain)
        grain = new GrainNode;
    std::unique_ptr<QSGTexture> previous;
    if (!grain->texture || grain->dark != m_dark) {
        static const QImage darkTile = tile(true);
        static const QImage lightTile = tile(false);
        previous = std::move(grain->texture);
        grain->texture.reset(window()->createTextureFromImage(m_dark ? darkTile : lightTile));
        grain->dark = m_dark;
    }

    const auto ratio = window()->effectiveDevicePixelRatio();
    const auto step = tileSize / ratio;
    const auto columns = static_cast<int>(std::ceil(bounds.width() / step));
    const auto rows = static_cast<int>(std::ceil(bounds.height() / step));
    const auto count = static_cast<size_t>(columns) * static_cast<size_t>(rows);
    while (grain->tiles.size() > count) {
        auto* patch = grain->tiles.back();
        grain->tiles.pop_back();
        grain->removeChildNode(patch);
        delete patch;
    }
    const auto existing = grain->tiles.size();
    while (grain->tiles.size() < count) {
        auto* patch = window()->createImageNode();
        patch->setFiltering(QSGTexture::Nearest);
        grain->tiles.push_back(patch);
    }
    for (int row = 0; row < rows; ++row) {
        for (int column = 0; column < columns; ++column) {
            const QRectF target(column * step, row * step, std::min(step, bounds.width() - column * step),
                std::min(step, bounds.height() - row * step));
            auto* patch = grain->tiles[static_cast<size_t>(row) * static_cast<size_t>(columns)
                + static_cast<size_t>(column)];
            patch->setTexture(grain->texture.get());
            patch->setSourceRect(QRectF(0, 0, target.width() * ratio, target.height() * ratio));
            patch->setRect(target);
        }
    }
    for (auto index = existing; index < grain->tiles.size(); ++index)
        grain->appendChildNode(grain->tiles[index]);
    return grain;
}

void Grain::geometryChange(const QRectF& next, const QRectF& previous)
{
    QQuickItem::geometryChange(next, previous);
    if (next.size() != previous.size())
        update();
}

void Grain::itemChange(const ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged || change == ItemSceneChange)
        update();
}

}
