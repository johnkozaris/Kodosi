#include "terminal/TerminalImages.hpp"

#include "terminal/TerminalPlaceholders.hpp"

#include <algorithm>
#include <climits>
#include <iterator>
#include <utility>
#include <vector>

namespace kodosi {
namespace {

// The formats match the rasterizer's target, so drawing an image is a plain blend.
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

TerminalImagePlacement::Layer layer(const std::int32_t z)
{
    if (z < INT32_MIN / 2) {
        return TerminalImagePlacement::Layer::BelowBackground;
    }
    return z < 0 ? TerminalImagePlacement::Layer::BelowText
                 : TerminalImagePlacement::Layer::AboveText;
}

// Ghostty draws the images of placeholder cells below the text.
constexpr std::int32_t placeholderZ = -1;

struct Target {
    std::uint32_t imageId = 0;
    std::uint32_t placementId = 0;
    std::uint32_t columns = 0;
    std::uint32_t rows = 0;
    bool isVirtual = false;
};

// The placement whose grid a run fills. The C interface does not tell whether the
// program chose a placement id, so a virtual placement and then the lowest id win.
const Target* target(const std::vector<Target>& targets, const PlaceholderRun& run)
{
    const Target* best = nullptr;
    for (const auto& candidate : targets) {
        if (candidate.imageId != run.imageId) {
            continue;
        }
        if (run.placementId != 0) {
            if (candidate.placementId == run.placementId
                && (best == nullptr || (candidate.isVirtual && !best->isVirtual))) {
                best = &candidate;
            }
        } else if (candidate.isVirtual
            && (best == nullptr || candidate.placementId < best->placementId)) {
            best = &candidate;
        }
    }
    return best;
}

}

QVector<TerminalImagePlacement> TerminalImages::capture(
    const GhosttyTerminal terminal,
    const GhosttyRenderState renderState)
{
    auto previous = std::exchange(m_pixels, {});
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
    std::uint64_t generation = 0;
    if (ghostty_terminal_get_multi(terminal, std::size(keys), keys, values, nullptr)
            != GHOSTTY_SUCCESS
        || graphics == nullptr || columns == 0 || rows == 0
        || ghostty_kitty_graphics_get(graphics, GHOSTTY_KITTY_GRAPHICS_DATA_GENERATION, &generation)
            != GHOSTTY_SUCCESS
        || generation == 0) {
        return {};
    }
    // Without the host's cell size an image covers no cells, as on the host.
    const auto cellWidth = widthPixels / columns;
    const auto cellHeight = heightPixels / rows;
    if (cellWidth == 0 || cellHeight == 0 || !iteratePlacements(graphics)) {
        return {};
    }

    struct Placed {
        TerminalImagePlacement placement;
        std::int32_t z;
        std::uint32_t imageId;
    };
    std::vector<Placed> placed;
    std::vector<Target> targets;
    auto hasVirtual = false;
    while (ghostty_kitty_graphics_placement_next(m_iterator.get())) {
        Target placement;
        std::uint32_t xOffset = 0;
        std::uint32_t yOffset = 0;
        std::int32_t z = 0;
        const GhosttyKittyGraphicsPlacementData placementKeys[] {
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET,
            GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z,
        };
        void* placementValues[] {
            &placement.imageId,
            &placement.placementId,
            &placement.isVirtual,
            &placement.columns,
            &placement.rows,
            &xOffset,
            &yOffset,
            &z,
        };
        if (ghostty_kitty_graphics_placement_get_multi(
                m_iterator.get(),
                std::size(placementKeys),
                placementKeys,
                placementValues,
                nullptr)
            != GHOSTTY_SUCCESS) {
            continue;
        }
        targets.push_back(placement);
        hasVirtual = hasVirtual || placement.isVirtual;
        const auto imageId = placement.imageId;
        if (placement.isVirtual) {
            continue;
        }
        const auto image = ghostty_kitty_graphics_image(graphics, imageId);
        GhosttyKittyGraphicsPlacementRenderInfo info =
            GHOSTTY_INIT_SIZED(GhosttyKittyGraphicsPlacementRenderInfo);
        if (image == nullptr
            || ghostty_kitty_graphics_placement_render_info(m_iterator.get(), image, terminal, &info)
                != GHOSTTY_SUCCESS
            || !info.viewport_visible || info.pixel_width == 0 || info.pixel_height == 0) {
            continue;
        }
        auto pixels = this->pixels(image, previous);
        if (pixels.isNull()) {
            continue;
        }
        const auto left = std::min(xOffset, cellWidth - 1);
        const auto top = std::min(yOffset, cellHeight - 1);
        placed.push_back({
            .placement = {
                .pixels = std::move(pixels),
                .source = QRect(
                    static_cast<int>(info.source_x),
                    static_cast<int>(info.source_y),
                    static_cast<int>(info.source_width),
                    static_cast<int>(info.source_height)),
                .bounds = QRectF(
                    info.viewport_col + (static_cast<qreal>(left) / cellWidth),
                    info.viewport_row + (static_cast<qreal>(top) / cellHeight),
                    static_cast<qreal>(info.pixel_width) / cellWidth,
                    static_cast<qreal>(info.pixel_height) / cellHeight),
                .layer = layer(z),
            },
            .z = z,
            .imageId = imageId,
        });
    }

    const auto runs = hasVirtual ? placeholderRuns(renderState) : std::vector<PlaceholderRun> {};
    for (const auto& run : runs) {
        const auto* placement = target(targets, run);
        const auto image = ghostty_kitty_graphics_image(graphics, run.imageId);
        if (placement == nullptr || image == nullptr) {
            continue;
        }
        auto pixels = this->pixels(image, previous);
        if (pixels.isNull()) {
            continue;
        }
        const auto drawing = placeholderDrawing(
            run, placement->columns, placement->rows, pixels.size(), cellWidth, cellHeight);
        if (!drawing) {
            continue;
        }
        placed.push_back({
            .placement = {
                .pixels = std::move(pixels),
                .source = drawing->source,
                .bounds = QRectF(
                    run.column + (static_cast<qreal>(drawing->offset.x()) / cellWidth),
                    run.row + (static_cast<qreal>(drawing->offset.y()) / cellHeight),
                    static_cast<qreal>(drawing->size.width()) / cellWidth,
                    static_cast<qreal>(drawing->size.height()) / cellHeight),
                .layer = layer(placeholderZ),
            },
            .z = placeholderZ,
            .imageId = run.imageId,
        });
    }

    // Ghostty's order: the z-index, then the image id.
    std::ranges::stable_sort(placed, {}, [](const Placed& entry) {
        return std::pair {entry.z, entry.imageId};
    });
    QVector<TerminalImagePlacement> placements;
    placements.reserve(static_cast<qsizetype>(placed.size()));
    for (auto& entry : placed) {
        placements.push_back(std::move(entry.placement));
    }
    return placements;
}

bool TerminalImages::iteratePlacements(const GhosttyKittyGraphics graphics)
{
    if (m_iterator == nullptr) {
        GhosttyKittyGraphicsPlacementIterator iterator = nullptr;
        if (ghostty_kitty_graphics_placement_iterator_new(nullptr, &iterator) != GHOSTTY_SUCCESS) {
            return false;
        }
        m_iterator.reset(iterator);
    }
    auto* iterator = m_iterator.get();
    return ghostty_kitty_graphics_get(
               graphics,
               GHOSTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
               &iterator)
        == GHOSTTY_SUCCESS;
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
    // A pending image, whose pixels a snapshot left out, has no data yet.
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
