#pragma once

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
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

template <typename Value>
struct TerminalRange {
    Value lowest;
    Value highest;

    [[nodiscard]] constexpr Value clamp(const Value value) const noexcept
    {
        return std::clamp(value, lowest, highest);
    }
    [[nodiscard]] constexpr bool holds(const Value value) const noexcept
    {
        return value >= lowest && value <= highest;
    }
};

inline constexpr TerminalRange<int> terminalFontPixels {8, 32};
inline constexpr TerminalRange<double> terminalLineHeights {0.8, 2.0};
inline constexpr TerminalRange<int> terminalScrollbackLines {100, 100'000};

enum class TerminalCursorStyle {
    Block,
    Bar,
    Underline,
    HollowBlock,
};

enum class TerminalLook {
    Dark,
    Light,
};

struct TerminalRgb {
    std::uint8_t red;
    std::uint8_t green;
    std::uint8_t blue;
};

struct TerminalColors {
    TerminalRgb foreground;
    TerminalRgb background;
    TerminalRgb cursor;
    TerminalRgb selectionBackground;
    TerminalRgb selectionForeground;
    std::array<TerminalRgb, 16> ansi;
    double minimumContrast;
};

inline constexpr TerminalColors darkTerminalColors {
    .foreground = {0xe8, 0xe1, 0xd9},
    .background = {0x0d, 0x0b, 0x09},
    .cursor = {0xcc, 0x7a, 0x45},
    .selectionBackground = {0xdb, 0x8a, 0x62},
    .selectionForeground = {0x16, 0x0e, 0x0a},
    .ansi = {{
        {0x1a, 0x16, 0x13}, {0xc4, 0x40, 0x40}, {0x52, 0xb3, 0x6b}, {0xc4, 0x99, 0x3d},
        {0x5d, 0x8e, 0xc7}, {0xb0, 0x6d, 0xba}, {0x5d, 0xaf, 0xa6}, {0xc4, 0xb8, 0xa8},
        {0x8c, 0x7a, 0x6f}, {0xd6, 0x60, 0x60}, {0x6d, 0xcc, 0x85}, {0xd6, 0xb3, 0x5d},
        {0x7d, 0xaa, 0xd9}, {0xc8, 0x8d, 0xd0}, {0x7d, 0xc7, 0xbe}, {0xe8, 0xe1, 0xd9},
    }},
    .minimumContrast = 1.1,
};

// White and bright white are mid grays, so that text a program draws as white stays readable.
inline constexpr TerminalColors lightTerminalColors {
    .foreground = {0x2a, 0x1d, 0x15},
    .background = {0xfa, 0xf6, 0xee},
    .cursor = {0xb8, 0x5a, 0x26},
    .selectionBackground = {0xf0, 0xdf, 0xcb},
    .selectionForeground = {0x2a, 0x1d, 0x15},
    .ansi = {{
        {0x2a, 0x1d, 0x15}, {0xb2, 0x3a, 0x32}, {0x2f, 0x7d, 0x54}, {0x7e, 0x5e, 0x12},
        {0x2e, 0x64, 0xa8}, {0x8f, 0x3f, 0x9c}, {0x1f, 0x7a, 0x74}, {0x7d, 0x6b, 0x5f},
        {0x5e, 0x4d, 0x42}, {0xc4, 0x45, 0x3b}, {0x3a, 0x8f, 0x60}, {0x94, 0x70, 0x1a},
        {0x3a, 0x74, 0xbe}, {0xa2, 0x4d, 0xb0}, {0x2a, 0x8f, 0x88}, {0x9a, 0x87, 0x78},
    }},
    .minimumContrast = 3.0,
};

[[nodiscard]] constexpr const TerminalColors& terminalColors(const TerminalLook look) noexcept
{
    return look == TerminalLook::Light ? lightTerminalColors : darkTerminalColors;
}

struct TerminalKernelSettings {
    TerminalCursorStyle cursorStyle = TerminalCursorStyle::Block;
    bool cursorBlink = false;
    std::size_t scrollbackLines = 10'000;
    std::size_t scrollbackBytes = terminalScrollbackByteBudget(scrollbackLines);
    TerminalLook look = TerminalLook::Dark;

    bool operator==(const TerminalKernelSettings&) const = default;
};

}
