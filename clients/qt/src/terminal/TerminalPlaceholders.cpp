#include "terminal/TerminalPlaceholders.hpp"

#include <QScopeGuard>
#include <QVarLengthArray>

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <span>

namespace kodosi {
namespace {

constexpr std::uint32_t placeholder = 0x10EEEE;

// A cell with more marks than this cannot be a placeholder that a program wrote.
constexpr std::uint32_t maximumPlaceholderCodepoints = 64;

// The row and column diacritics of the Kitty graphics protocol, sorted. The index
// of a diacritic is its value.
constexpr std::array<std::uint32_t, 297> diacritics {
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F,
    0x0346, 0x034A, 0x034B, 0x034C, 0x0350, 0x0351, 0x0352, 0x0357,
    0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484,
    0x0485, 0x0486, 0x0487, 0x0592, 0x0593, 0x0594, 0x0595, 0x0597,
    0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611,
    0x0612, 0x0613, 0x0614, 0x0615, 0x0616, 0x0617, 0x0657, 0x0658,
    0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2,
    0x06E4, 0x06E7, 0x06E8, 0x06EB, 0x06EC, 0x0730, 0x0732, 0x0733,
    0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE,
    0x07EF, 0x07F0, 0x07F1, 0x07F3, 0x0816, 0x0817, 0x0818, 0x0819,
    0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C,
    0x082D, 0x0951, 0x0953, 0x0954, 0x0F82, 0x0F83, 0x0F86, 0x0F87,
    0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D,
    0x1B6E, 0x1B6F, 0x1B70, 0x1B71, 0x1B72, 0x1B73, 0x1CD0, 0x1CD1,
    0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1,
    0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5, 0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9,
    0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1,
    0x20D4, 0x20D5, 0x20D6, 0x20D7, 0x20DB, 0x20DC, 0x20E1, 0x20E7,
    0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA,
    0x2DEB, 0x2DEC, 0x2DED, 0x2DEE, 0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2,
    0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D,
    0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1, 0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5,
    0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7,
    0xAAB8, 0xAABE, 0xAABF, 0xAAC1, 0xFE20, 0xFE21, 0xFE22, 0xFE23,
    0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186, 0x1D187,
    0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243,
    0x1D244,
};

std::optional<std::uint32_t> diacriticValue(const std::uint32_t codepoint)
{
    const auto found = std::ranges::lower_bound(diacritics, codepoint);
    if (found == diacritics.end() || *found != codepoint) {
        return std::nullopt;
    }
    return static_cast<std::uint32_t>(found - diacritics.begin());
}

std::uint32_t colorId(const GhosttyStyleColor& color)
{
    switch (color.tag) {
    case GHOSTTY_STYLE_COLOR_PALETTE:
        return color.value.palette;
    case GHOSTTY_STYLE_COLOR_RGB:
        return (std::uint32_t {color.value.rgb.r} << 16) | (std::uint32_t {color.value.rgb.g} << 8)
            | color.value.rgb.b;
    default:
        return 0;
    }
}

// One placeholder cell. A missing value continues the run that the cell joins.
struct PlaceholderCell {
    std::uint32_t imageIdLow = 0;
    std::optional<std::uint32_t> imageIdHigh = std::nullopt;
    std::optional<std::uint32_t> placementId = std::nullopt;
    std::optional<std::uint32_t> imageRow = std::nullopt;
    std::optional<std::uint32_t> imageColumn = std::nullopt;
};

PlaceholderCell decode(const GhosttyStyle& style, const std::span<const std::uint32_t> marks)
{
    PlaceholderCell cell {.imageIdLow = colorId(style.fg_color)};
    if (const auto placementId = colorId(style.underline_color); placementId != 0) {
        cell.placementId = placementId;
    }
    if (!marks.empty()) {
        cell.imageRow = diacriticValue(marks[0]);
    }
    if (marks.size() > 1) {
        cell.imageColumn = diacriticValue(marks[1]);
    }
    if (marks.size() > 2) {
        if (const auto high = diacriticValue(marks[2]); high && *high <= 0xFF) {
            cell.imageIdHigh = high;
        }
    }
    return cell;
}

struct OpenRun {
    PlaceholderCell first;
    PlaceholderRun run;

    [[nodiscard]] bool takes(const PlaceholderCell& cell) const
    {
        return cell.imageIdLow == first.imageIdLow && cell.placementId == first.placementId
            && (!cell.imageRow || *cell.imageRow == run.imageRow)
            && (!cell.imageColumn || *cell.imageColumn == run.imageColumn + run.width)
            && (!cell.imageIdHigh || cell.imageIdHigh == first.imageIdHigh);
    }

    [[nodiscard]] PlaceholderRun complete() const
    {
        auto completed = run;
        completed.imageId = first.imageIdLow | (first.imageIdHigh.value_or(0) << 24);
        completed.placementId = first.placementId.value_or(0);
        return completed;
    }
};

OpenRun open(const PlaceholderCell& cell, const std::uint16_t column, const std::uint16_t row)
{
    return {
        .first = cell,
        .run = {
            .column = column,
            .row = row,
            .imageColumn = cell.imageColumn.value_or(0),
            .imageRow = cell.imageRow.value_or(0),
        },
    };
}

std::optional<PlaceholderCell> placeholderCell(const GhosttyRenderStateRowCells cells)
{
    GhosttyCell raw = 0;
    std::uint32_t codepoint = 0;
    if (ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, &raw)
            != GHOSTTY_SUCCESS
        || ghostty_cell_get(raw, GHOSTTY_CELL_DATA_CODEPOINT, &codepoint) != GHOSTTY_SUCCESS
        || codepoint != placeholder) {
        return std::nullopt;
    }
    std::uint32_t length = 0;
    GhosttyStyle style = GHOSTTY_INIT_SIZED(GhosttyStyle);
    if (ghostty_render_state_row_cells_get(
            cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN, &length)
            != GHOSTTY_SUCCESS
        || length == 0 || length > maximumPlaceholderCodepoints
        || ghostty_render_state_row_cells_get(cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &style)
            != GHOSTTY_SUCCESS) {
        return std::nullopt;
    }
    QVarLengthArray<std::uint32_t, 4> codepoints(static_cast<qsizetype>(length));
    if (ghostty_render_state_row_cells_get(
            cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, codepoints.data())
        != GHOSTTY_SUCCESS) {
        return std::nullopt;
    }
    return decode(
        style,
        std::span<const std::uint32_t>(codepoints.constData(), length).subspan(1));
}

}

std::vector<PlaceholderRun> placeholderRuns(const GhosttyRenderState renderState)
{
    GhosttyRenderStateRowIterator rows = nullptr;
    GhosttyRenderStateRowCells cells = nullptr;
    const auto release = qScopeGuard([&] {
        ghostty_render_state_row_cells_free(cells);
        ghostty_render_state_row_iterator_free(rows);
    });
    if (ghostty_render_state_row_iterator_new(nullptr, &rows) != GHOSTTY_SUCCESS
        || ghostty_render_state_get(renderState, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &rows)
            != GHOSTTY_SUCCESS
        || ghostty_render_state_row_cells_new(nullptr, &cells) != GHOSTTY_SUCCESS) {
        return {};
    }

    std::vector<PlaceholderRun> runs;
    for (std::uint16_t row = 0; ghostty_render_state_row_iterator_next(rows); ++row) {
        GhosttyRow raw = 0;
        bool placeholders = false;
        if (ghostty_render_state_row_get(rows, GHOSTTY_RENDER_STATE_ROW_DATA_RAW, &raw)
                != GHOSTTY_SUCCESS
            || ghostty_row_get(raw, GHOSTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER, &placeholders)
                != GHOSTTY_SUCCESS
            || !placeholders
            || ghostty_render_state_row_get(rows, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &cells)
                != GHOSTTY_SUCCESS) {
            continue;
        }
        std::optional<OpenRun> run;
        for (std::uint16_t column = 0; ghostty_render_state_row_cells_next(cells); ++column) {
            const auto cell = placeholderCell(cells);
            if (run && cell && run->takes(*cell)) {
                ++run->run.width;
                continue;
            }
            if (run) {
                runs.push_back(run->complete());
                run.reset();
            }
            if (cell) {
                run = open(*cell, column, row);
            }
        }
        if (run) {
            runs.push_back(run->complete());
        }
    }
    return runs;
}

std::optional<PlaceholderDrawing> placeholderDrawing(
    const PlaceholderRun& run,
    const std::uint32_t gridColumns,
    const std::uint32_t gridRows,
    const QSize image,
    const std::uint32_t cellWidth,
    const std::uint32_t cellHeight)
{
    if (image.isEmpty() || cellWidth == 0 || cellHeight == 0) {
        return std::nullopt;
    }
    const auto imageWidth = static_cast<std::uint64_t>(image.width());
    const auto imageHeight = static_cast<std::uint64_t>(image.height());
    const auto columns =
        gridColumns != 0 ? std::uint64_t {gridColumns} : (imageWidth + cellWidth - 1) / cellWidth;
    const auto rows =
        gridRows != 0 ? std::uint64_t {gridRows} : (imageHeight + cellHeight - 1) / cellHeight;
    constexpr auto maximumGrid = std::uint64_t {std::numeric_limits<std::uint16_t>::max()};
    if (columns > maximumGrid || rows > maximumGrid) {
        return std::nullopt;
    }

    // Fit the image into the grid and keep its aspect ratio, centered.
    const auto width = static_cast<double>(imageWidth);
    const auto height = static_cast<double>(imageHeight);
    const auto gridWidth = static_cast<double>(columns * cellWidth);
    const auto gridHeight = static_cast<double>(rows * cellHeight);
    double scale = 0;
    double marginX = 0;
    double marginY = 0;
    if (width * gridHeight > height * gridWidth) {
        scale = gridWidth / width;
        marginY = (gridHeight - (height * scale)) / 2 / scale;
    } else {
        scale = gridHeight / height;
        marginX = (gridWidth - (width * scale)) / 2 / scale;
    }

    // The run's part of the image with its margins, in image pixels.
    const auto paddedWidth = width + (marginX * 2);
    const auto paddedHeight = height + (marginY * 2);
    const auto columnsF = static_cast<double>(columns);
    const auto rowsF = static_cast<double>(rows);
    auto sourceX = paddedWidth * (static_cast<double>(run.imageColumn) / columnsF);
    auto sourceY = paddedHeight * (static_cast<double>(run.imageRow) / rowsF);
    auto sourceWidth = paddedWidth * (static_cast<double>(run.width) / columnsF);
    auto sourceHeight = paddedHeight * (1.0 / rowsF);

    // Cut the margins away, since the image has no pixels there.
    const auto cut = [scale](
                         double& source,
                         double& length,
                         double& offset,
                         double& target,
                         const double margin,
                         const double padded,
                         const double full) {
        if (source < margin) {
            const auto inside = margin - source;
            length -= inside;
            offset = inside;
            target -= inside * scale;
            source = 0;
            if (length > full) {
                length = full;
                target = full * scale;
            }
        } else if (source + length > padded - margin) {
            source -= margin;
            length = padded - margin - source - margin;
            target = length * scale;
        } else {
            source -= margin;
        }
    };
    double offsetX = 0;
    double offsetY = 0;
    auto targetWidth = static_cast<double>(std::uint64_t {run.width} * cellWidth);
    auto targetHeight = static_cast<double>(cellHeight);
    cut(sourceY, sourceHeight, offsetY, targetHeight, marginY, paddedHeight, height);
    cut(sourceX, sourceWidth, offsetX, targetWidth, marginX, paddedWidth, width);
    if (sourceWidth <= 0 || sourceHeight <= 0) {
        return std::nullopt;
    }

    const auto round = [](const double value) { return static_cast<int>(std::lround(value)); };
    PlaceholderDrawing drawing {
        .source = QRect(round(sourceX), round(sourceY), round(sourceWidth), round(sourceHeight)),
        .offset = QPoint(round(offsetX * scale), round(offsetY * scale)),
        .size = QSize(round(targetWidth), round(targetHeight)),
    };
    if (drawing.size.isEmpty()) {
        return std::nullopt;
    }
    return drawing;
}

}
