#pragma once

#include <QColor>
#include <QString>
#include <QVector>

#include <cstdint>

namespace kodosi {

struct TerminalCell {
    QString grapheme;
    QColor foreground;
    QColor background;
    bool bold = false;
    bool italic = false;
    bool faint = false;
    bool inverse = false;
    bool invisible = false;
    bool strikethrough = false;
    bool overline = false;
    bool selected = false;
    std::int32_t underline = 0;
    std::uint8_t width = 1;
};

struct TerminalCursor {
    bool visible = false;
    bool blinking = false;
    bool passwordInput = false;
    bool wideTail = false;
    std::uint16_t column = 0;
    std::uint16_t row = 0;
    std::int32_t visualStyle = 0;
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
