#include "terminal/TerminalView.hpp"

#include <QQuickWindow>

#include <algorithm>
#include <cmath>

namespace kodosi {

QString TerminalView::accessibleText() const
{
    if (!m_frame) {
        return {};
    }
    QString text;
    text.reserve(static_cast<qsizetype>(m_frame->rows) * (m_frame->columns + 1));
    for (std::uint16_t row = 0; row < m_frame->rows; ++row) {
        for (std::uint16_t column = 0; column < m_frame->columns; ++column) {
            const auto* cell = m_frame->cell(column, row);
            if (cell != nullptr && cell->width != 0) {
                text += cell->grapheme.isEmpty() ? QStringLiteral(" ") : cell->grapheme;
            }
        }
        if (row + 1 < m_frame->rows) {
            text += QLatin1Char('\n');
        }
    }
    return text;
}

int TerminalView::accessibleCursorPosition() const
{
    if (!m_frame) {
        return 0;
    }
    int offset = 0;
    for (std::uint16_t row = 0; row < m_frame->rows; ++row) {
        for (std::uint16_t column = 0; column < m_frame->columns; ++column) {
            if (row == m_frame->cursor.row && column == m_frame->cursor.column) {
                return offset;
            }
            const auto* cell = m_frame->cell(column, row);
            if (cell != nullptr && cell->width != 0) {
                offset += std::max(1, static_cast<int>(cell->grapheme.size()));
            }
        }
        if (row + 1 < m_frame->rows) {
            ++offset;
        }
    }
    return offset;
}

QPair<int, int> TerminalView::accessibleSelection() const
{
    if (!m_frame) {
        return {-1, -1};
    }
    int offset = 0;
    int start = -1;
    int end = -1;
    for (std::uint16_t row = 0; row < m_frame->rows; ++row) {
        for (std::uint16_t column = 0; column < m_frame->columns; ++column) {
            const auto* cell = m_frame->cell(column, row);
            if (cell == nullptr || cell->width == 0) {
                continue;
            }
            const auto length = std::max(1, static_cast<int>(cell->grapheme.size()));
            if (cell->selected) {
                start = start < 0 ? offset : start;
                end = offset + length;
            }
            offset += length;
        }
        if (row + 1 < m_frame->rows) {
            ++offset;
        }
    }
    return {start, end};
}

QRect TerminalView::accessibleCharacterRect(const int offset) const
{
    if (!m_frame || window() == nullptr || offset < 0) {
        return {};
    }
    int current = 0;
    std::uint16_t targetColumn = 0;
    std::uint16_t targetRow = 0;
    std::uint8_t targetWidth = 1;
    bool found = false;
    for (std::uint16_t row = 0; row < m_frame->rows && !found; ++row) {
        for (std::uint16_t column = 0; column < m_frame->columns; ++column) {
            const auto* cell = m_frame->cell(column, row);
            if (cell == nullptr || cell->width == 0) {
                continue;
            }
            const auto length = std::max(1, static_cast<int>(cell->grapheme.size()));
            if (offset >= current && offset < current + length) {
                targetColumn = column;
                targetRow = row;
                targetWidth = std::max<std::uint8_t>(cell->width, 1);
                found = true;
                break;
            }
            current += length;
        }
        if (row + 1 < m_frame->rows) {
            ++current;
        }
    }
    if (!found) {
        return {};
    }
    const auto cellSize = TerminalRasterizer::cellSize(m_font, m_lineHeight);
    const auto scale = viewportScale();
    const auto scenePoint = mapToScene(viewportOffset() +
        QPointF(cellSize.width() * targetColumn, cellSize.height() * targetRow) * scale);
    return {
        window()->mapToGlobal(scenePoint.toPoint()),
        QSize(
            std::max(1, static_cast<int>(std::ceil(cellSize.width() * targetWidth * scale))),
            std::max(1, static_cast<int>(std::ceil(cellSize.height() * scale)))),
    };
}

int TerminalView::accessibleOffsetAt(const QPoint& globalPoint) const
{
    if (!m_frame || window() == nullptr) {
        return -1;
    }
    const auto scenePoint = window()->mapFromGlobal(globalPoint);
    const auto localPoint = mapFromScene(scenePoint);
    auto cell = terminalCellAt(localPoint);
    if (!cell) {
        return -1;
    }
    if (const auto* value = m_frame->cell(
            static_cast<std::uint16_t>(cell->x()),
            static_cast<std::uint16_t>(cell->y()));
        value != nullptr && value->width == 0 && cell->x() > 0) {
        cell->rx() -= 1;
    }
    int offset = 0;
    for (int row = 0; row <= cell->y(); ++row) {
        const auto endColumn = row == cell->y() ? cell->x() : m_frame->columns;
        for (int column = 0; column < endColumn; ++column) {
            const auto* value = m_frame->cell(
                static_cast<std::uint16_t>(column),
                static_cast<std::uint16_t>(row));
            if (value != nullptr && value->width != 0) {
                offset += std::max(1, static_cast<int>(value->grapheme.size()));
            }
        }
        if (row < cell->y()) {
            ++offset;
        }
    }
    return offset;
}

}
