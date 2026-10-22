#include "terminal/TerminalRasterizer.hpp"

#include <QFontMetricsF>
#include <QHashFunctions>
#include <QPainter>
#include <QPainterPath>
#include <QStringView>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <tuple>
#include <utility>
#include <vector>

namespace kodosi {
namespace {

constexpr int maximumTextureDimension = 16'384;
constexpr std::uint64_t maximumImageBytes = 256ULL * 1024ULL * 1024ULL;
constexpr QStringView kittyPlaceholder = u"\U0010EEEE";

struct CellMetrics {
    qreal width;
    qreal height;
    qreal ascent;
};

CellMetrics metrics(const QFont& font, const qreal lineHeight, const qreal devicePixelRatio)
{
    // Whole device pixels keep every cell on one grid at fractional display scales.
    const auto ratio = devicePixelRatio > 0.0 && std::isfinite(devicePixelRatio)
        ? devicePixelRatio
        : 1.0;
    const auto devicePixels = [ratio](const qreal value) {
        return std::ceil(value * ratio) / ratio;
    };
    const QFontMetricsF fontMetrics(font);
    const auto baseHeight = devicePixels(fontMetrics.height());
    const auto height = devicePixels(baseHeight * std::clamp(lineHeight, 0.8, 2.0));
    return {
        devicePixels(fontMetrics.horizontalAdvance(QLatin1Char('M'))),
        height,
        std::round((fontMetrics.ascent() + ((height - baseHeight) / 2.0)) * ratio) / ratio,
    };
}

// The edges of a block fall on whole device pixels counted from the corner of its cell, so a block
// has the same pixels in every row, and a row moved in the image matches the row painted there.
void fillBlock(
    QPainter& painter,
    const QRectF& bounds,
    const QColor& color,
    const QRectF& part,
    const qreal opacity = 1.0)
{
    auto shade = color;
    shade.setAlphaF(color.alphaF() * opacity);
    const auto ratio = painter.device()->devicePixelRatio();
    const auto edge = [ratio](const qreal origin, const qreal size, const qreal fraction) {
        return (std::round(origin * ratio) + std::round(std::round(size * ratio) * fraction)) / ratio;
    };
    painter.fillRect(
        QRectF(
            QPointF(
                edge(bounds.left(), bounds.width(), part.left()),
                edge(bounds.top(), bounds.height(), part.top())),
            QPointF(
                edge(bounds.left(), bounds.width(), part.right()),
                edge(bounds.top(), bounds.height(), part.bottom()))),
        shade);
}

void drawBlockElement(
    QPainter& painter,
    const QRectF& bounds,
    const char16_t codepoint,
    const QColor& color)
{
    static constexpr std::array<std::uint8_t, 10> quadrants {4, 8, 1, 13, 9, 7, 11, 2, 6, 14};
    if (codepoint == 0x2580) {
        fillBlock(painter, bounds, color, {0.0, 0.0, 1.0, 0.5});
    } else if (codepoint <= 0x2588) {
        const auto eighths = codepoint - 0x2580;
        fillBlock(painter, bounds, color, {0.0, 1.0 - (eighths / 8.0), 1.0, eighths / 8.0});
    } else if (codepoint <= 0x258F) {
        fillBlock(painter, bounds, color, {0.0, 0.0, (0x2590 - codepoint) / 8.0, 1.0});
    } else if (codepoint == 0x2590) {
        fillBlock(painter, bounds, color, {0.5, 0.0, 0.5, 1.0});
    } else if (codepoint <= 0x2593) {
        fillBlock(painter, bounds, color, {0.0, 0.0, 1.0, 1.0}, (codepoint - 0x2590) / 4.0);
    } else if (codepoint == 0x2594) {
        fillBlock(painter, bounds, color, {0.0, 0.0, 1.0, 0.125});
    } else if (codepoint == 0x2595) {
        fillBlock(painter, bounds, color, {0.875, 0.0, 0.125, 1.0});
    } else {
        const auto mask = quadrants.at(static_cast<std::size_t>(codepoint - 0x2596));
        for (const auto& [bit, part] : {
                 std::pair {1, QRectF {0.0, 0.0, 0.5, 0.5}},
                 std::pair {2, QRectF {0.5, 0.0, 0.5, 0.5}},
                 std::pair {4, QRectF {0.0, 0.5, 0.5, 0.5}},
                 std::pair {8, QRectF {0.5, 0.5, 0.5, 0.5}},
             }) {
            if ((mask & bit) != 0) {
                fillBlock(painter, bounds, color, part);
            }
        }
    }
}

void drawPowerlineSeparator(
    QPainter& painter,
    const QRectF& bounds,
    const char16_t codepoint,
    const QColor& color)
{
    const auto left = bounds.left();
    const auto top = bounds.top();
    const auto right = bounds.right();
    const auto bottom = bounds.bottom();
    const auto middle = bounds.center().y();
    const QRectF rightBulge(left - bounds.width(), top, bounds.width() * 2.0, bounds.height());
    const QRectF leftBulge(right - bounds.width(), top, bounds.width() * 2.0, bounds.height());
    const auto polygon = [](const std::initializer_list<QPointF> points, const bool closed) {
        QPainterPath path(*points.begin());
        for (auto point = points.begin() + 1; point != points.end(); ++point) {
            path.lineTo(*point);
        }
        if (closed) {
            path.closeSubpath();
        }
        return path;
    };
    QPainterPath path;
    auto outline = false;
    switch (codepoint) {
    case 0xE0B0:
    case 0xE0B1:
        outline = codepoint == 0xE0B1;
        path = polygon({{left, top}, {right, middle}, {left, bottom}}, !outline);
        break;
    case 0xE0B2:
    case 0xE0B3:
        outline = codepoint == 0xE0B3;
        path = polygon({{right, top}, {left, middle}, {right, bottom}}, !outline);
        break;
    case 0xE0B4:
    case 0xE0B5:
        outline = codepoint == 0xE0B5;
        path.arcMoveTo(rightBulge, 90.0);
        path.arcTo(rightBulge, 90.0, -180.0);
        break;
    case 0xE0B6:
    case 0xE0B7:
        outline = codepoint == 0xE0B7;
        path.arcMoveTo(leftBulge, 90.0);
        path.arcTo(leftBulge, 90.0, 180.0);
        break;
    case 0xE0B8:
        path = polygon({{left, top}, {right, bottom}, {left, bottom}}, true);
        break;
    case 0xE0BA:
        path = polygon({{right, top}, {right, bottom}, {left, bottom}}, true);
        break;
    case 0xE0BC:
        path = polygon({{left, top}, {right, top}, {left, bottom}}, true);
        break;
    case 0xE0BE:
        path = polygon({{left, top}, {right, top}, {right, bottom}}, true);
        break;
    case 0xE0B9:
    case 0xE0BF:
        outline = true;
        path = polygon({{left, top}, {right, bottom}}, false);
        break;
    default:
        outline = true;
        path = polygon({{left, bottom}, {right, top}}, false);
        break;
    }
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);
    if (outline) {
        painter.setPen(QPen(
            color,
            std::max(1.0, bounds.height() / 16.0),
            Qt::SolidLine,
            Qt::FlatCap,
            Qt::MiterJoin));
        painter.setBrush(Qt::NoBrush);
    } else {
        painter.setPen(Qt::NoPen);
        painter.setBrush(color);
    }
    painter.drawPath(path);
    painter.restore();
}

// Block elements and Powerline separators fill their cell exactly, whatever font
// would draw them, so neighbouring cells join without gaps or overhang.
bool drawCellShape(
    QPainter& painter,
    const QRectF& bounds,
    const QString& grapheme,
    const QColor& color)
{
    if (grapheme.size() != 1) {
        return false;
    }
    const auto codepoint = grapheme.front().unicode();
    if (codepoint >= 0x2580 && codepoint <= 0x259F) {
        drawBlockElement(painter, bounds, codepoint, color);
        return true;
    }
    if (codepoint >= 0xE0B0 && codepoint <= 0xE0BF) {
        drawPowerlineSeparator(painter, bounds, codepoint, color);
        return true;
    }
    return false;
}

void drawImages(
    QPainter& painter,
    const TerminalFrame& frame,
    const CellMetrics& cell,
    const TerminalImagePlacement::Layer layer)
{
    for (const auto& image : frame.images) {
        if (image.layer != layer) {
            continue;
        }
        painter.drawImage(
            QRectF(
                image.bounds.x() * cell.width,
                image.bounds.y() * cell.height,
                image.bounds.width() * cell.width,
                image.bounds.height() * cell.height),
            image.pixels,
            image.source);
    }
}

// The four faces of the font. Each cell that starts a run of another face sets the painter's
// font, so a row of plain text sets it once.
struct Faces {
    explicit Faces(const QFont& font)
        : regular(font)
        , bold(font)
        , italic(font)
        , boldItalic(font)
    {
        regular.setBold(false);
        regular.setItalic(false);
        bold.setBold(true);
        bold.setItalic(false);
        italic.setBold(false);
        italic.setItalic(true);
        boldItalic.setBold(true);
        boldItalic.setItalic(true);
    }

    [[nodiscard]] const QFont& of(const TerminalCell& cell) const
    {
        if (cell.bold) {
            return cell.italic ? boldItalic : bold;
        }
        return cell.italic ? italic : regular;
    }

    QFont regular;
    QFont bold;
    QFont italic;
    QFont boldItalic;
};

void drawDecoration(
    QPainter& painter,
    const QRectF& bounds,
    const TerminalCell& cell,
    const QColor& foreground)
{
    painter.setPen(QPen(foreground, 1.0));
    if (cell.underline != 0) {
        const auto y = bounds.bottom() - 1.0;
        painter.drawLine(QPointF(bounds.left(), y), QPointF(bounds.right(), y));
        if (cell.underline == 2) {
            painter.drawLine(QPointF(bounds.left(), y - 2.0), QPointF(bounds.right(), y - 2.0));
        }
    }
    if (cell.strikethrough) {
        painter.drawLine(
            QPointF(bounds.left(), bounds.center().y()),
            QPointF(bounds.right(), bounds.center().y()));
    }
    if (cell.overline) {
        painter.drawLine(
            QPointF(bounds.left(), bounds.top() + 1.0),
            QPointF(bounds.right(), bounds.top() + 1.0));
    }
}

QRectF rowBounds(const TerminalFrame& frame, const CellMetrics& cell, const std::uint16_t row)
{
    return {0.0, cell.height * row, cell.width * frame.columns, cell.height};
}

void drawRowBackgrounds(
    QPainter& painter,
    const TerminalFrame& frame,
    const CellMetrics& cell,
    const TerminalRasterizer::Palette& palette,
    const std::uint16_t row)
{
    for (std::uint16_t column = 0; column < frame.columns; ++column) {
        const auto* terminalCell = frame.cell(column, row);
        if (terminalCell == nullptr) {
            continue;
        }
        const auto background = terminalCell->selected
            ? palette.selectionBackground
            : terminalCell->background;
        if (background != frame.background) {
            painter.fillRect(
                QRectF(cell.width * column, cell.height * row, cell.width, cell.height),
                background);
        }
    }
}

void drawRowText(
    QPainter& painter,
    const TerminalFrame& frame,
    const CellMetrics& cell,
    const Faces& faces,
    const TerminalRasterizer::Palette& palette,
    const std::uint16_t row)
{
    const QFont* face = nullptr;
    for (std::uint16_t column = 0; column < frame.columns; ++column) {
        const auto* terminalCell = frame.cell(column, row);
        if (terminalCell == nullptr) {
            continue;
        }
        if (terminalCell->width == 0 || terminalCell->grapheme.isEmpty()
            || terminalCell->grapheme.startsWith(kittyPlaceholder)) {
            continue;
        }
        const auto span = std::max<std::uint8_t>(terminalCell->width, 1);
        const QRectF bounds(
            cell.width * column,
            cell.height * row,
            cell.width * span,
            cell.height);
        const auto foreground = terminalCell->selected
            ? palette.selectionForeground
            : terminalCell->foreground;
        if (const auto& next = faces.of(*terminalCell); face != &next) {
            face = &next;
            painter.setFont(next);
        }
        painter.setPen(foreground);
        if (!drawCellShape(painter, bounds, terminalCell->grapheme, foreground)) {
            painter.drawText(
                QPointF(bounds.left(), bounds.top() + cell.ascent),
                terminalCell->grapheme);
        }
        drawDecoration(painter, bounds, *terminalCell, foreground);
    }
}

void drawPreedit(
    QPainter& painter,
    const TerminalFrame& frame,
    const CellMetrics& cell,
    const QFont& font,
    const TerminalRasterizer::Options& options)
{
    if (options.overlay.preedit.isEmpty()
        || options.overlay.column >= frame.columns
        || options.overlay.row >= frame.rows) {
        return;
    }
    const QRectF preeditBounds(
        cell.width * options.overlay.column,
        cell.height * options.overlay.row,
        cell.width * std::max<qsizetype>(options.overlay.preedit.size(), 1),
        cell.height);
    painter.fillRect(preeditBounds, options.palette.preeditBackground);
    painter.setFont(font);
    painter.setPen(options.palette.preeditForeground);
    painter.drawText(
        QPointF(preeditBounds.left(), preeditBounds.top() + cell.ascent),
        options.overlay.preedit);
    painter.drawLine(
        QPointF(preeditBounds.left(), preeditBounds.bottom() - 1.0),
        QPointF(preeditBounds.right(), preeditBounds.bottom() - 1.0));
}

bool cursorDrawn(const TerminalFrame& frame, const TerminalRasterizer::Options& options)
{
    return frame.cursor.visible && options.cursorPhaseVisible
        && frame.cursor.column < frame.columns && frame.cursor.row < frame.rows;
}

TerminalCursorStyle cursorStyle(
    const TerminalFrame& frame,
    const TerminalRasterizer::Options& options)
{
    return options.cursorStyle.value_or(
        frame.cursor.visualStyle == 0
            ? TerminalCursorStyle::Bar
            : frame.cursor.visualStyle == 2
              ? TerminalCursorStyle::Underline
              : frame.cursor.visualStyle == 3
                ? TerminalCursorStyle::HollowBlock
              : TerminalCursorStyle::Block);
}

void drawCursor(
    QPainter& painter,
    const TerminalFrame& frame,
    const CellMetrics& cell,
    const TerminalRasterizer::Options& options)
{
    if (!cursorDrawn(frame, options)) {
        return;
    }
    auto cursorColumn = frame.cursor.column;
    if (frame.cursor.wideTail && cursorColumn > 0) {
        --cursorColumn;
    }
    const auto* cursorCell = frame.cell(cursorColumn, frame.cursor.row);
    const auto cursorSpan = cursorCell != nullptr && cursorCell->width == 2
        ? std::uint8_t {2}
        : std::uint8_t {1};
    const QRectF cursorBounds(
        cell.width * cursorColumn,
        cell.height * frame.cursor.row,
        cell.width * cursorSpan,
        cell.height);
    painter.setPen(QPen(frame.cursorColor, 1.0));
    switch (cursorStyle(frame, options)) {
    case TerminalCursorStyle::Bar:
        painter.fillRect(
            QRectF(cursorBounds.left(), cursorBounds.top(), 2.0, cursorBounds.height()),
            frame.cursorColor);
        break;
    case TerminalCursorStyle::Underline:
        painter.fillRect(
            QRectF(cursorBounds.left(), cursorBounds.bottom() - 2.0, cursorBounds.width(), 2.0),
            frame.cursorColor);
        break;
    case TerminalCursorStyle::HollowBlock:
        painter.drawRect(cursorBounds.adjusted(0.5, 0.5, -0.5, -0.5));
        break;
    case TerminalCursorStyle::Block: {
        auto cursor = frame.cursorColor;
        cursor.setAlpha(150);
        painter.fillRect(cursorBounds, cursor);
        break;
    }
    }
}

bool sameRow(
    const TerminalFrame& frame,
    const std::uint16_t row,
    const TerminalFrame& earlier,
    const std::uint16_t earlierRow)
{
    const auto first = static_cast<qsizetype>(row) * frame.columns;
    const auto earlierFirst = static_cast<qsizetype>(earlierRow) * earlier.columns;
    return std::equal(
        frame.cells.cbegin() + first,
        frame.cells.cbegin() + first + frame.columns,
        earlier.cells.cbegin() + earlierFirst);
}

std::vector<std::size_t> rowHashes(const TerminalFrame& frame)
{
    std::vector<std::size_t> hashes(frame.rows, 0);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        auto seed = std::size_t {0};
        for (std::uint16_t column = 0; column < frame.columns; ++column) {
            const auto& cell = frame.cells[static_cast<qsizetype>(row) * frame.columns + column];
            seed = qHashMulti(seed, cell.grapheme, cell.foreground.rgba(), cell.background.rgba(),
                cell.bold, cell.italic, cell.invisible, cell.strikethrough, cell.overline,
                cell.selected, cell.underline, cell.width);
        }
        hashes[row] = seed;
    }
    return hashes;
}

// How far the rows moved: row r of the frame shows row r + shift of the earlier frame. A move
// counts only when it keeps at least two more rows than no move, so it needs at least three rows
// that changed in place.
int scrollOf(const TerminalFrame& earlier, const TerminalFrame& frame)
{
    auto changedInPlace = 0;
    for (std::uint16_t row = 0; row < frame.rows && changedInPlace < 3; ++row) {
        changedInPlace += sameRow(frame, row, earlier, row) ? 0 : 1;
    }
    if (changedInPlace < 3) {
        return 0;
    }
    const auto before = rowHashes(earlier);
    const auto after = rowHashes(frame);
    const int rows = frame.rows;
    const auto kept = [&](const int shift) {
        auto count = 0;
        for (auto row = std::max(0, -shift); row < std::min(rows, rows - shift); ++row) {
            count += after[row] == before[row + shift] ? 1 : 0;
        }
        return count;
    };
    auto best = 0;
    auto bestKept = kept(0) + 1;
    for (auto shift = 1 - rows; shift < rows; ++shift) {
        if (const auto count = shift == 0 ? 0 : kept(shift); count > bestKept) {
            best = shift;
            bestKept = count;
        }
    }
    return best;
}

void moveRows(QImage& image, const int shift, const int rowPixels)
{
    const auto offset = static_cast<qsizetype>(std::abs(shift)) * rowPixels * image.bytesPerLine();
    if (offset >= image.sizeInBytes()) {
        return;
    }
    auto* bits = image.bits();
    const auto length = static_cast<std::size_t>(image.sizeInBytes() - offset);
    if (shift > 0) {
        std::memmove(bits, bits + offset, length);
    } else {
        std::memmove(bits + offset, bits, length);
    }
}

// What a row needs: its own pixels painted again, or also the rows next to it, which its glyphs
// can reach into.
enum class RowChange : std::uint8_t {
    None,
    Pixels,
    Glyphs,
};

// The rows whose pixels can differ between two frames, once the rows of the earlier image have
// moved by shift: rows with other cells or with no earlier row and the rows of the text being
// composed when it changes, with the rows next to them; an edge row that lost the text reaching in
// from beyond the edge, the rows of the cursor when it changes, and the rows under changed images.
std::vector<RowChange> changedRows(
    const TerminalFrame& earlier,
    const TerminalRasterizer::Options& earlierOptions,
    const TerminalFrame& frame,
    const TerminalRasterizer::Options& options,
    const int shift)
{
    std::vector<RowChange> changed(frame.rows, RowChange::None);
    const auto mark = [&changed](const int row, const RowChange change) {
        if (row >= 0 && static_cast<std::size_t>(row) < changed.size()) {
            auto& current = changed[static_cast<std::size_t>(row)];
            current = std::max(current, change);
        }
    };
    const int rows = frame.rows;
    for (auto row = 0; row < rows; ++row) {
        const auto source = row + shift;
        if (source < 0 || source >= rows
            || !sameRow(frame, static_cast<std::uint16_t>(row), earlier,
                static_cast<std::uint16_t>(source))) {
            mark(row, RowChange::Glyphs);
        }
    }
    if (shift > 0) {
        mark(0, RowChange::Pixels);
    } else if (shift < 0) {
        mark(rows - 1, RowChange::Pixels);
    }
    const auto earlierCursor = cursorDrawn(earlier, earlierOptions);
    const auto cursor = cursorDrawn(frame, options);
    if (shift != 0 || earlierCursor != cursor || earlier.cursor != frame.cursor
        || earlier.cursorColor != frame.cursorColor
        || cursorStyle(earlier, earlierOptions) != cursorStyle(frame, options)) {
        if (earlierCursor) {
            mark(earlier.cursor.row - shift, RowChange::Pixels);
        }
        if (cursor) {
            mark(frame.cursor.row, RowChange::Pixels);
        }
    }
    if (shift != 0 || earlierOptions.overlay != options.overlay) {
        if (!earlierOptions.overlay.preedit.isEmpty()) {
            mark(earlierOptions.overlay.row - shift, RowChange::Glyphs);
        }
        if (!options.overlay.preedit.isEmpty()) {
            mark(options.overlay.row, RowChange::Glyphs);
        }
    }
    const auto moved = [&earlier, shift] {
        auto images = earlier.images;
        for (auto& image : images) {
            image.bounds.translate(0.0, -shift);
        }
        return images;
    }();
    if (moved != frame.images) {
        const auto markImage = [&](const TerminalImagePlacement& image) {
            const auto top = std::max(0, static_cast<int>(std::floor(image.bounds.top())));
            const auto bottom = std::min(rows, static_cast<int>(std::ceil(image.bounds.bottom())));
            for (auto row = top; row < bottom; ++row) {
                mark(row, RowChange::Pixels);
            }
        };
        QVector<TerminalImagePlacement> kept;
        QVector<TerminalImagePlacement> keeps;
        for (const auto& [these, those, common] : {
                 std::tuple {&moved, &frame.images, &kept},
                 std::tuple {&frame.images, &moved, &keeps},
             }) {
            for (const auto& image : *these) {
                if (those->contains(image)) {
                    common->push_back(image);
                } else {
                    markImage(image);
                }
            }
        }
        // Images that stay but change their order overlap in another order.
        if (kept != keeps) {
            std::ranges::for_each(kept, markImage);
        }
    }
    return changed;
}

}

QSizeF TerminalRasterizer::logicalSize(const TerminalFrame& frame, const QFont& font)
{
    return logicalSize(frame, font, 1.0);
}

QSizeF TerminalRasterizer::logicalSize(
    const TerminalFrame& frame,
    const QFont& font,
    const qreal lineHeight,
    const qreal devicePixelRatio)
{
    const auto cell = metrics(font, lineHeight, devicePixelRatio);
    return {
        cell.width * frame.columns,
        cell.height * frame.rows,
    };
}

QSizeF TerminalRasterizer::cellSize(const QFont& font)
{
    return cellSize(font, 1.0);
}

QSizeF TerminalRasterizer::cellSize(
    const QFont& font,
    const qreal lineHeight,
    const qreal devicePixelRatio)
{
    const auto cell = metrics(font, lineHeight, devicePixelRatio);
    return {cell.width, cell.height};
}

QImage TerminalRasterizer::render(
    const TerminalFrame& frame,
    const QFont& font,
    const qreal devicePixelRatio)
{
    return render(frame, font, devicePixelRatio, Options {});
}

QImage TerminalRasterizer::render(
    const TerminalFrame& frame,
    const QFont& font,
    const qreal devicePixelRatio,
    const Palette& palette)
{
    Options options;
    options.palette = palette;
    return render(frame, font, devicePixelRatio, options);
}

QImage TerminalRasterizer::render(
    const TerminalFrame& frame,
    const QFont& font,
    const qreal devicePixelRatio,
    const Palette& palette,
    const Overlay& overlay)
{
    Options options;
    options.palette = palette;
    options.overlay = overlay;
    return render(frame, font, devicePixelRatio, options);
}

QImage TerminalRasterizer::render(
    const TerminalFrame& frame,
    const QFont& font,
    const qreal devicePixelRatio,
    const Options& options)
{
    if (frame.columns == 0 || frame.rows == 0 || devicePixelRatio <= 0.0
        || !std::isfinite(devicePixelRatio)) {
        return {};
    }

    const auto cell = metrics(font, options.lineHeight, devicePixelRatio);
    const auto logicalWidth = cell.width * frame.columns;
    const auto logicalHeight = cell.height * frame.rows;
    const auto physicalWidth = std::round(logicalWidth * devicePixelRatio);
    const auto physicalHeight = std::round(logicalHeight * devicePixelRatio);
    if (physicalWidth <= 0.0 || physicalHeight <= 0.0
        || physicalWidth > maximumTextureDimension
        || physicalHeight > maximumTextureDimension) {
        return {};
    }
    const auto width = static_cast<int>(physicalWidth);
    const auto height = static_cast<int>(physicalHeight);
    const auto imageBytes = static_cast<std::uint64_t>(width)
        * static_cast<std::uint64_t>(height) * 4ULL;
    if (imageBytes > maximumImageBytes) {
        return {};
    }

    QImage image(width, height, QImage::Format_RGBA8888_Premultiplied);
    if (image.isNull()) {
        return {};
    }
    image.setDevicePixelRatio(devicePixelRatio);
    image.fill(frame.background);

    const Faces faces(font);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::TextAntialiasing, true);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    drawImages(painter, frame, cell, TerminalImagePlacement::Layer::BelowBackground);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        drawRowBackgrounds(painter, frame, cell, options.palette, row);
    }
    drawImages(painter, frame, cell, TerminalImagePlacement::Layer::BelowText);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        drawRowText(painter, frame, cell, faces, options.palette, row);
    }
    drawImages(painter, frame, cell, TerminalImagePlacement::Layer::AboveText);
    drawPreedit(painter, frame, cell, font, options);
    drawCursor(painter, frame, cell, options);
    painter.end();
    return image;
}

// Glyphs can reach into the next row, so each painted row also draws the text of its
// neighbours, clipped to itself, and a changed row paints its neighbours too.
std::optional<int> TerminalRasterizer::update(
    QImage& image,
    const TerminalFrame& earlier,
    const Options& earlierOptions,
    const TerminalFrame& frame,
    const QFont& font,
    const Options& options)
{
    const auto devicePixelRatio = image.devicePixelRatio();
    const auto cells = static_cast<qsizetype>(frame.columns) * frame.rows;
    if (image.isNull() || frame.columns == 0 || frame.rows == 0
        || frame.columns != earlier.columns || frame.rows != earlier.rows
        || frame.cells.size() != cells || earlier.cells.size() != cells
        || frame.background != earlier.background
        || options.palette != earlierOptions.palette
        || options.lineHeight != earlierOptions.lineHeight) {
        return std::nullopt;
    }
    const auto cell = metrics(font, options.lineHeight, devicePixelRatio);
    if (std::round(cell.width * frame.columns * devicePixelRatio) != image.width()
        || std::round(cell.height * frame.rows * devicePixelRatio) != image.height()) {
        return std::nullopt;
    }

    const auto shift = scrollOf(earlier, frame);
    if (shift != 0) {
        moveRows(image, shift, static_cast<int>(std::lround(cell.height * devicePixelRatio)));
    }
    const auto changed = changedRows(earlier, earlierOptions, frame, options, shift);
    std::vector<bool> painted(frame.rows, false);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        if (changed[row] != RowChange::None) {
            painted[row] = true;
        }
        if (changed[row] == RowChange::Glyphs) {
            painted[row == 0 ? 0 : row - 1] = true;
            painted[std::min<std::uint16_t>(row + 1, frame.rows - 1)] = true;
        }
    }
    const auto count = static_cast<int>(std::count(painted.cbegin(), painted.cend(), true));
    if (count == 0) {
        return 0;
    }

    const Faces faces(font);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::TextAntialiasing, true);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        if (!painted[row]) {
            continue;
        }
        const auto bounds = rowBounds(frame, cell, row);
        painter.setClipRect(bounds);
        painter.setCompositionMode(QPainter::CompositionMode_Source);
        painter.fillRect(bounds, frame.background);
        painter.setCompositionMode(QPainter::CompositionMode_SourceOver);
        drawImages(painter, frame, cell, TerminalImagePlacement::Layer::BelowBackground);
        drawRowBackgrounds(painter, frame, cell, options.palette, row);
        drawImages(painter, frame, cell, TerminalImagePlacement::Layer::BelowText);
        const auto first = row == 0 ? std::uint16_t {0} : static_cast<std::uint16_t>(row - 1);
        const auto last = std::min<std::uint16_t>(row + 1, frame.rows - 1);
        for (auto reach = first; reach <= last; ++reach) {
            drawRowText(painter, frame, cell, faces, options.palette, reach);
        }
        drawImages(painter, frame, cell, TerminalImagePlacement::Layer::AboveText);
        drawPreedit(painter, frame, cell, font, options);
        drawCursor(painter, frame, cell, options);
    }
    painter.end();
    return count;
}

}
