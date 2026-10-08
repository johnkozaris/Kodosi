#include "terminal/TerminalRasterizer.hpp"

#include <QFontMetricsF>
#include <QPainter>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <limits>

namespace kodosi {
namespace {

constexpr int maximumTextureDimension = 16'384;
constexpr std::uint64_t maximumImageBytes = 256ULL * 1024ULL * 1024ULL;

struct CellMetrics {
    qreal width;
    qreal height;
    qreal ascent;
};

CellMetrics metrics(const QFont& font, const qreal lineHeight)
{
    const QFontMetricsF fontMetrics(font);
    const auto baseHeight = std::ceil(fontMetrics.height());
    const auto height = std::ceil(baseHeight * std::clamp(lineHeight, 0.8, 2.0));
    return {
        std::ceil(fontMetrics.horizontalAdvance(QLatin1Char('M'))),
        height,
        fontMetrics.ascent() + ((height - baseHeight) / 2.0),
    };
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
    const qreal lineHeight)
{
    const auto cell = metrics(font, lineHeight);
    return {
        cell.width * frame.columns,
        cell.height * frame.rows,
    };
}

QSizeF TerminalRasterizer::cellSize(const QFont& font)
{
    return cellSize(font, 1.0);
}

QSizeF TerminalRasterizer::cellSize(const QFont& font, const qreal lineHeight)
{
    const auto cell = metrics(font, lineHeight);
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

    const auto cell = metrics(font, options.lineHeight);
    const auto logicalWidth = cell.width * frame.columns;
    const auto logicalHeight = cell.height * frame.rows;
    const auto physicalWidth = std::ceil(logicalWidth * devicePixelRatio);
    const auto physicalHeight = std::ceil(logicalHeight * devicePixelRatio);
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
            painter.drawText(
                QPointF(bounds.left(), bounds.top() + cell.ascent),
                terminalCell->grapheme);
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
