#pragma once

#include "terminal/TerminalFrame.hpp"
#include "terminal/TerminalSettings.hpp"

#include <QFont>
#include <QImage>
#include <QSizeF>

#include <optional>

namespace kodosi {

class TerminalRasterizer final {
public:
    struct Palette {
        QColor selectionBackground {219, 138, 98};
        QColor selectionForeground {22, 14, 10};
        QColor preeditBackground {219, 138, 98};
        QColor preeditForeground {22, 14, 10};
    };

    struct Overlay {
        QString preedit;
        std::uint16_t column = 0;
        std::uint16_t row = 0;
    };

    struct Options {
        Palette palette;
        Overlay overlay;
        qreal lineHeight = 1.0;
        std::optional<TerminalCursorStyle> cursorStyle;
        bool cursorPhaseVisible = true;
    };

    [[nodiscard]] static QSizeF logicalSize(
        const TerminalFrame& frame,
        const QFont& font);
    [[nodiscard]] static QSizeF logicalSize(
        const TerminalFrame& frame,
        const QFont& font,
        qreal lineHeight);
    [[nodiscard]] static QSizeF cellSize(const QFont& font);
    [[nodiscard]] static QSizeF cellSize(const QFont& font, qreal lineHeight);
    [[nodiscard]] static QImage render(
        const TerminalFrame& frame,
        const QFont& font,
        qreal devicePixelRatio);
    [[nodiscard]] static QImage render(
        const TerminalFrame& frame,
        const QFont& font,
        qreal devicePixelRatio,
        const Palette& palette);
    [[nodiscard]] static QImage render(
        const TerminalFrame& frame,
        const QFont& font,
        qreal devicePixelRatio,
        const Palette& palette,
        const Overlay& overlay);
    [[nodiscard]] static QImage render(
        const TerminalFrame& frame,
        const QFont& font,
        qreal devicePixelRatio,
        const Options& options);
};

}
