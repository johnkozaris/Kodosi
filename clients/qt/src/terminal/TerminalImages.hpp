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
        void operator()(GhosttyRenderStateImageIterator iterator) const noexcept
        {
            ghostty_render_state_image_iterator_free(iterator);
        }
    };

    [[nodiscard]] QImage pixels(GhosttyKittyGraphicsImage image, Cache& previous);

    Cache m_pixels;
    std::unique_ptr<std::remove_pointer_t<GhosttyRenderStateImageIterator>, IteratorFree>
        m_iterator;
    std::uint64_t m_generation = 0;
    QVector<TerminalImagePlacement> m_placements;
};

}
