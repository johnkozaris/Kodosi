#include "terminal/TerminalImages.hpp"

#include <algorithm>
#include <iterator>
#include <tuple>
#include <utility>
#include <vector>

namespace kodosi {
namespace {

QImage convert(
    const std::uint8_t* const data,
    const std::size_t length,
    const std::uint32_t width,
    const std::uint32_t height,
    const GhosttyKittyImageFormat format)
{
    const auto pixels = static_cast<std::size_t>(width) * height;
    const auto converted = [&](
                               const std::size_t bytesPerPixel,
                               const QImage::Format source,
                               const QImage::Format target) {
        if (length != pixels * bytesPerPixel) {
            return QImage {};
        }
        return QImage(
            data,
            static_cast<int>(width),
            static_cast<int>(height),
            static_cast<qsizetype>(width * bytesPerPixel),
            source)
            .convertToFormat(target);
    };
    switch (format) {
    case GHOSTTY_KITTY_IMAGE_FORMAT_RGB:
        return converted(3, QImage::Format_RGB888, QImage::Format_RGBX8888);
    case GHOSTTY_KITTY_IMAGE_FORMAT_RGBA:
        return converted(4, QImage::Format_RGBA8888, QImage::Format_RGBA8888_Premultiplied);
    case GHOSTTY_KITTY_IMAGE_FORMAT_GRAY:
        return converted(1, QImage::Format_Grayscale8, QImage::Format_RGBX8888);
    case GHOSTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA:
        break;
    default:
        return {};
    }
    if (length != pixels * 2) {
        return {};
    }
    QImage image(
        static_cast<int>(width),
        static_cast<int>(height),
        QImage::Format_RGBA8888_Premultiplied);
    if (image.isNull()) {
        return {};
    }
    for (std::uint32_t y = 0; y < height; ++y) {
        const auto* source = data + (static_cast<std::size_t>(y) * width * 2);
        auto* target = image.scanLine(static_cast<int>(y));
        for (std::uint32_t x = 0; x < width; ++x) {
            const auto alpha = source[(x * 2) + 1];
            const auto gray = static_cast<std::uint8_t>(((source[x * 2] * alpha) + 127) / 255);
            std::fill_n(target + (x * 4), 3, gray);
            target[(x * 4) + 3] = alpha;
        }
    }
    return image;
}

TerminalImagePlacement::Layer layer(const GhosttyRenderStateImageLayer value)
{
    switch (value) {
    case GHOSTTY_RENDER_STATE_IMAGE_LAYER_BELOW_BACKGROUND:
        return TerminalImagePlacement::Layer::BelowBackground;
    case GHOSTTY_RENDER_STATE_IMAGE_LAYER_BELOW_TEXT:
        return TerminalImagePlacement::Layer::BelowText;
    default:
        return TerminalImagePlacement::Layer::AboveText;
    }
}

}

QVector<TerminalImagePlacement> TerminalImages::capture(
    const GhosttyTerminal terminal,
    const GhosttyRenderState renderState)
{
    std::uint64_t generation = 0;
    if (ghostty_render_state_get(
            renderState, GHOSTTY_RENDER_STATE_DATA_IMAGE_PLACEMENTS_GENERATION, &generation)
            != GHOSTTY_SUCCESS
        || generation == 0) {
        m_pixels.clear();
        m_placements.clear();
        m_generation = 0;
        return {};
    }
    if (generation == m_generation) {
        return m_placements;
    }
    auto previous = std::exchange(m_pixels, {});
    m_placements.clear();
    m_generation = 0;
    GhosttyKittyGraphics graphics = nullptr;
    std::uint16_t columns = 0;
    std::uint16_t rows = 0;
    std::uint32_t widthPixels = 0;
    std::uint32_t heightPixels = 0;
    const GhosttyTerminalData keys[] {
        GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS,
        GHOSTTY_TERMINAL_DATA_COLS,
        GHOSTTY_TERMINAL_DATA_ROWS,
        GHOSTTY_TERMINAL_DATA_WIDTH_PX,
        GHOSTTY_TERMINAL_DATA_HEIGHT_PX,
    };
    void* values[] {&graphics, &columns, &rows, &widthPixels, &heightPixels};
    if (ghostty_terminal_get_multi(terminal, std::size(keys), keys, values, nullptr)
            != GHOSTTY_SUCCESS
        || graphics == nullptr || columns == 0 || rows == 0) {
        return {};
    }
    const auto cellWidth = static_cast<qreal>(widthPixels / columns);
    const auto cellHeight = static_cast<qreal>(heightPixels / rows);
    if (cellWidth == 0 || cellHeight == 0) {
        return {};
    }
    if (m_iterator == nullptr) {
        GhosttyRenderStateImageIterator iterator = nullptr;
        if (ghostty_render_state_image_iterator_new(nullptr, &iterator) != GHOSTTY_SUCCESS) {
            return {};
        }
        m_iterator.reset(iterator);
    }
    auto* iterator = m_iterator.get();
    if (ghostty_render_state_get(renderState, GHOSTTY_RENDER_STATE_DATA_IMAGE_ITERATOR, &iterator)
        != GHOSTTY_SUCCESS) {
        return {};
    }

    struct Placed {
        TerminalImagePlacement placement;
        std::int32_t z;
        std::uint32_t imageId;
    };
    std::vector<Placed> placed;
    while (ghostty_render_state_image_iterator_next(iterator)) {
        auto protocol = GHOSTTY_IMAGE_PROTOCOL_KITTY;
        auto drawLayer = GHOSTTY_RENDER_STATE_IMAGE_LAYER_ABOVE_TEXT;
        std::uint32_t imageId = 0;
        std::int32_t z = 0;
        std::int32_t column = 0;
        std::int32_t row = 0;
        std::uint32_t offsetX = 0;
        std::uint32_t offsetY = 0;
        std::uint32_t width = 0;
        std::uint32_t height = 0;
        std::uint32_t sourceX = 0;
        std::uint32_t sourceY = 0;
        std::uint32_t sourceWidth = 0;
        std::uint32_t sourceHeight = 0;
        const GhosttyRenderStateImageData placementKeys[] {
            GHOSTTY_RENDER_STATE_IMAGE_DATA_PROTOCOL,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_IMAGE_ID,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_LAYER,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_Z,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_COL,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_VIEWPORT_ROW,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_X,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_CELL_OFFSET_Y,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_WIDTH,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_PIXEL_HEIGHT,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_X,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_Y,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_WIDTH,
            GHOSTTY_RENDER_STATE_IMAGE_DATA_SOURCE_HEIGHT,
        };
        void* placementValues[] {
            &protocol,
            &imageId,
            &drawLayer,
            &z,
            &column,
            &row,
            &offsetX,
            &offsetY,
            &width,
            &height,
            &sourceX,
            &sourceY,
            &sourceWidth,
            &sourceHeight,
        };
        if (ghostty_render_state_image_get_multi(
                iterator,
                std::size(placementKeys),
                placementKeys,
                placementValues,
                nullptr)
                != GHOSTTY_SUCCESS
            || protocol != GHOSTTY_IMAGE_PROTOCOL_KITTY || width == 0 || height == 0) {
            continue;
        }
        const auto image = ghostty_kitty_graphics_image(graphics, imageId);
        if (image == nullptr) {
            continue;
        }
        auto pixels = this->pixels(image, previous);
        if (pixels.isNull()) {
            continue;
        }
        placed.push_back({
            .placement = {
                .pixels = std::move(pixels),
                .source = QRect(
                    static_cast<int>(sourceX),
                    static_cast<int>(sourceY),
                    static_cast<int>(sourceWidth),
                    static_cast<int>(sourceHeight)),
                .bounds = QRectF(
                    column + (offsetX / cellWidth),
                    row + (offsetY / cellHeight),
                    width / cellWidth,
                    height / cellHeight),
                .layer = layer(drawLayer),
            },
            .z = z,
            .imageId = imageId,
        });
    }

    std::ranges::stable_sort(placed, {}, [](const Placed& entry) {
        return std::tuple {entry.placement.layer, entry.z, entry.imageId};
    });
    m_placements.reserve(static_cast<qsizetype>(placed.size()));
    for (auto& entry : placed) {
        m_placements.push_back(std::move(entry.placement));
    }
    m_generation = generation;
    return m_placements;
}

QImage TerminalImages::pixels(const GhosttyKittyGraphicsImage image, Cache& previous)
{
    std::uint32_t id = 0;
    std::uint64_t generation = 0;
    const GhosttyKittyGraphicsImageData identity[] {
        GHOSTTY_KITTY_IMAGE_DATA_ID,
        GHOSTTY_KITTY_IMAGE_DATA_GENERATION,
    };
    void* identityValues[] {&id, &generation};
    if (ghostty_kitty_graphics_image_get_multi(
            image, std::size(identity), identity, identityValues, nullptr)
        != GHOSTTY_SUCCESS) {
        return {};
    }
    if (const auto shown = m_pixels.find(id); shown != m_pixels.end()) {
        return shown->second.image;
    }
    if (const auto known = previous.find(id);
        known != previous.end() && known->second.generation == generation) {
        return m_pixels.emplace(id, std::move(known->second)).first->second.image;
    }
    std::uint32_t width = 0;
    std::uint32_t height = 0;
    auto format = GHOSTTY_KITTY_IMAGE_FORMAT_RGBA;
    std::size_t length = 0;
    const std::uint8_t* data = nullptr;
    const GhosttyKittyGraphicsImageData keys[] {
        GHOSTTY_KITTY_IMAGE_DATA_WIDTH,
        GHOSTTY_KITTY_IMAGE_DATA_HEIGHT,
        GHOSTTY_KITTY_IMAGE_DATA_FORMAT,
        GHOSTTY_KITTY_IMAGE_DATA_DATA_LEN,
        GHOSTTY_KITTY_IMAGE_DATA_DATA_PTR,
    };
    void* values[] {&width, &height, &format, &length, &data};
    if (ghostty_kitty_graphics_image_get_multi(image, std::size(keys), keys, values, nullptr)
            != GHOSTTY_SUCCESS
        || data == nullptr) {
        return {};
    }
    auto converted = convert(data, length, width, height, format);
    if (converted.isNull()) {
        return {};
    }
    return m_pixels.emplace(id, Pixels {generation, std::move(converted)}).first->second.image;
}

}
