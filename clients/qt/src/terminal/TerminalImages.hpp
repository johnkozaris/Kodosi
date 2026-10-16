#pragma once

#include "terminal/GhosttyC.hpp"
#include "terminal/TerminalFrame.hpp"

#include <cstdint>
#include <memory>
#include <type_traits>
#include <unordered_map>

namespace kodosi {

/// The Kitty images on the visible part of a terminal. An image's pixels are
/// converted once for each of its generations and kept while it is visible.
class TerminalImages final {
public:
    /// The placements in the viewport, measured in its cells. The host's cell size
    /// converts pixels to cells, so an image covers the same cells as on the host.
    [[nodiscard]] QVector<TerminalImagePlacement> capture(GhosttyTerminal terminal);

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
