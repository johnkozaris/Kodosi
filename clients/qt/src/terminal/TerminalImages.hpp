#pragma once

#include "terminal/GhosttyC.hpp"
#include "terminal/TerminalFrame.hpp"

#include <cstdint>
#include <memory>
#include <type_traits>
#include <unordered_map>

namespace kodosi {

class TerminalImages final {
public:
    [[nodiscard]] QVector<TerminalImagePlacement> capture(
        GhosttyTerminal terminal,
        GhosttyRenderState renderState);

private:
    struct Pixels {
        std::uint64_t generation = 0;
        QImage image;
    };
    using Cache = std::unordered_map<std::uint32_t, Pixels>;

    struct IteratorFree {
        void operator()(GhosttyKittyGraphicsPlacementIterator iterator) const noexcept
        {
            ghostty_kitty_graphics_placement_iterator_free(iterator);
        }
    };

    [[nodiscard]] bool iteratePlacements(GhosttyKittyGraphics graphics);
    [[nodiscard]] QImage pixels(GhosttyKittyGraphicsImage image, Cache& previous);

    Cache m_pixels;
    std::unique_ptr<std::remove_pointer_t<GhosttyKittyGraphicsPlacementIterator>, IteratorFree>
        m_iterator;
};

}
