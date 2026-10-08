#pragma once

#include <algorithm>
#include <cstddef>
#include <limits>

namespace kodosi {

[[nodiscard]] constexpr std::size_t terminalScrollbackByteBudget(
    const std::size_t lines) noexcept
{
    constexpr std::size_t bytesPerLine = 2'400;
    constexpr std::size_t pageBytes = 512 * 1'024;
    constexpr std::size_t minimumBytes = 2 * pageBytes;
    constexpr std::size_t maximumBytes = 256 * 1'024 * 1'024;
    const auto estimated = lines > std::numeric_limits<std::size_t>::max()
            / bytesPerLine
        ? maximumBytes
        : lines * bytesPerLine;
    const auto bounded =
        estimated < minimumBytes
        ? minimumBytes
        : estimated > maximumBytes ? maximumBytes : estimated;
    return std::min(
        ((bounded + pageBytes - 1) / pageBytes) * pageBytes,
        maximumBytes);
}

enum class TerminalCursorStyle {
    Block,
    Bar,
    Underline,
    HollowBlock,
};

struct TerminalKernelSettings {
    TerminalCursorStyle cursorStyle = TerminalCursorStyle::Block;
    bool cursorBlink = false;
    std::size_t scrollbackLines = 10'000;
    std::size_t scrollbackBytes = terminalScrollbackByteBudget(scrollbackLines);

    bool operator==(const TerminalKernelSettings&) const = default;
};

}
