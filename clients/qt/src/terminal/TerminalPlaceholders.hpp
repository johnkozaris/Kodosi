#pragma once

#include "terminal/GhosttyC.hpp"

#include <QPoint>
#include <QRect>
#include <QSize>

#include <cstdint>
#include <optional>
#include <vector>

namespace kodosi {

struct PlaceholderRun {
    std::uint16_t column = 0;
    std::uint16_t row = 0;
    std::uint32_t imageId = 0;
    std::uint32_t placementId = 0;
    std::uint32_t imageColumn = 0;
    std::uint32_t imageRow = 0;
    std::uint32_t width = 1;
};

[[nodiscard]] std::vector<PlaceholderRun> placeholderRuns(GhosttyRenderState renderState);

struct PlaceholderDrawing {
    QRect source;
    QPoint offset;
    QSize size;
};

[[nodiscard]] std::optional<PlaceholderDrawing> placeholderDrawing(
    const PlaceholderRun& run,
    std::uint32_t gridColumns,
    std::uint32_t gridRows,
    QSize image,
    std::uint32_t cellWidth,
    std::uint32_t cellHeight);

}
