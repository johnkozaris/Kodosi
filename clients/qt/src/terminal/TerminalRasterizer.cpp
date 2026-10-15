#include "terminal/TerminalRasterizer.hpp"

#include <QFontMetricsF>
#include <QPainter>
#include <QPainterPath>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <utility>

namespace kodosi {
namespace {

constexpr int maximumTextureDimension = 16'384;
constexpr std::uint64_t maximumImageBytes = 256ULL * 1024ULL * 1024ULL;

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

void fillBlock(
    QPainter& painter,
    const QRectF& bounds,
    const QColor& color,
    const QRectF& part,
    const qreal opacity = 1.0)
{
    auto shade = color;
    shade.setAlphaF(color.alphaF() * opacity);
    painter.fillRect(
        QRectF(
            bounds.left() + (bounds.width() * part.left()),
            bounds.top() + (bounds.height() * part.top()),
            bounds.width() * part.width(),
            bounds.height() * part.height()),
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

QFont styledFont(const QFont& base, const TerminalCell& cell)
{
    auto font = base;
    font.setBold(cell.bold);
    font.setItalic(cell.italic);
    return font;
}

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

    QPainter painter(&image);
    painter.setRenderHint(QPainter::TextAntialiasing, true);
    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        for (std::uint16_t column = 0; column < frame.columns; ++column) {
            const auto* terminalCell = frame.cell(column, row);
            if (terminalCell == nullptr) {
                continue;
            }
            const QRectF bounds(
                cell.width * column,
                cell.height * row,
                cell.width,
                cell.height);
            const auto background = terminalCell->selected
                ? options.palette.selectionBackground
                : terminalCell->background;
            if (background != frame.background) {
                painter.fillRect(bounds, background);
            }
        }
    }

    for (std::uint16_t row = 0; row < frame.rows; ++row) {
        for (std::uint16_t column = 0; column < frame.columns; ++column) {
            const auto* terminalCell = frame.cell(column, row);
            if (terminalCell == nullptr) {
                continue;
            }
            if (terminalCell->width == 0 || terminalCell->grapheme.isEmpty()) {
                continue;
            }
            const auto span = std::max<std::uint8_t>(terminalCell->width, 1);
            const QRectF bounds(
                cell.width * column,
                cell.height * row,
                cell.width * span,
                cell.height);
            const auto foreground = terminalCell->selected
                ? options.palette.selectionForeground
                : terminalCell->foreground;
            painter.setFont(styledFont(font, *terminalCell));
            painter.setPen(foreground);
            if (!drawCellShape(painter, bounds, terminalCell->grapheme, foreground)) {
                painter.drawText(
                    QPointF(bounds.left(), bounds.top() + cell.ascent),
                    terminalCell->grapheme);
            }
            drawDecoration(painter, bounds, *terminalCell, foreground);
        }
    }

    if (!options.overlay.preedit.isEmpty()
        && options.overlay.column < frame.columns
        && options.overlay.row < frame.rows) {
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

    if (frame.cursor.visible && options.cursorPhaseVisible
        && frame.cursor.column < frame.columns
        && frame.cursor.row < frame.rows) {
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
        const auto cursorStyle = options.cursorStyle.value_or(
            frame.cursor.visualStyle == 0
                ? TerminalCursorStyle::Bar
                : frame.cursor.visualStyle == 2
                  ? TerminalCursorStyle::Underline
                  : frame.cursor.visualStyle == 3
                    ? TerminalCursorStyle::HollowBlock
                  : TerminalCursorStyle::Block);
        switch (cursorStyle) {
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
    painter.end();
    return image;
}

}
