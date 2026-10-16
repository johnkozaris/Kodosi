#pragma once

#include "terminal/GhosttyC.hpp"

#include <QPoint>
#include <QRect>
#include <QSize>

#include <cstdint>
#include <optional>
#include <vector>

namespace kodosi {

/// Placeholder cells (U+10EEEE) side by side in one row that show one part of a
/// Kitty image's virtual placement. As in Ghostty, the foreground color gives the
/// image id, the underline color the placement id, and the diacritics the image
/// row, the image column and the high byte of the image id.
struct PlaceholderRun {
    std::uint16_t column = 0;
    std::uint16_t row = 0;
    std::uint32_t imageId = 0;
    std::uint32_t placementId = 0;
    std::uint32_t imageColumn = 0;
    std::uint32_t imageRow = 0;
    std::uint32_t width = 1;
};

/// The runs in the viewport of an updated render state.
[[nodiscard]] std::vector<PlaceholderRun> placeholderRuns(GhosttyRenderState renderState);

/// The part of an image that a run draws, in the terminal's pixels.
struct PlaceholderDrawing {
    QRect source;
    /// From the top-left corner of the run's first cell.
    QPoint offset;
    QSize size;
};

/// Fits the image into the grid of its virtual placement and crops the run's part,
/// as Ghostty does. A grid of zero columns or rows takes the size of the image.
[[nodiscard]] std::optional<PlaceholderDrawing> placeholderDrawing(
    const PlaceholderRun& run,
    std::uint32_t gridColumns,
    std::uint32_t gridRows,
    QSize image,
    std::uint32_t cellWidth,
    std::uint32_t cellHeight);

}
