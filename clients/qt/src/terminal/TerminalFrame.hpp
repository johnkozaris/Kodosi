#pragma once

#include <QColor>
#include <QImage>
#include <QRectF>
#include <QString>
#include <QVector>

#include <chrono>
#include <cstdint>
#include <optional>

namespace kodosi {

struct TerminalCell {
    QString grapheme;
    QColor foreground;
    QColor background;
    bool bold = false;
    bool italic = false;
    bool invisible = false;
    bool strikethrough = false;
    bool overline = false;
    bool selected = false;
    std::int32_t underline = 0;
    std::uint8_t width = 1;
    bool hyperlink = false;

    bool operator==(const TerminalCell&) const = default;
};

struct TerminalCursor {
    bool visible = false;
    bool blinking = false;
    bool wideTail = false;
    std::uint16_t column = 0;
    std::uint16_t row = 0;
    std::int32_t visualStyle = 0;

    bool operator==(const TerminalCursor&) const = default;
};

struct TerminalImagePlacement {
    enum class Layer : std::uint8_t { BelowBackground, BelowText, AboveText };

    QImage pixels;
    QRect source;
    QRectF bounds;
    Layer layer = Layer::AboveText;

    bool operator==(const TerminalImagePlacement& other) const
    {
        return pixels.cacheKey() == other.pixels.cacheKey() && source == other.source
            && bounds == other.bounds && layer == other.layer;
    }
};

struct TerminalScrollState {
    std::uint64_t totalRows = 0;
    std::uint64_t viewportOffset = 0;
    std::uint64_t viewportRows = 0;
};

struct TerminalFrame {
    std::uint16_t columns = 0;
    std::uint16_t rows = 0;
    std::uint64_t nextSequence = 0;
    std::uint64_t viewportRevision = 0;
    std::uint64_t displayRevision = 0;
    QColor foreground;
    QColor background;
    QColor cursorColor;
    TerminalCursor cursor;
    TerminalScrollState scroll;
    QVector<TerminalCell> cells;
    QVector<bool> wrappedRows;
    QVector<TerminalImagePlacement> images;
    std::optional<std::chrono::steady_clock::time_point> imageAnimationDue;
    bool mouseTracking = false;

    [[nodiscard]] const TerminalCell* cell(
        const std::uint16_t column,
        const std::uint16_t row) const noexcept
    {
        if (column >= columns || row >= rows) {
            return nullptr;
        }
        const auto index = static_cast<qsizetype>(row) * columns + column;
        return index < cells.size() ? &cells[index] : nullptr;
    }
};

}
