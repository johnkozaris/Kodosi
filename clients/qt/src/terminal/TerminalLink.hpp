#pragma once

#include <QString>

#include <cstdint>
#include <optional>

namespace kodosi {

struct TerminalFrame;

// Fallback plain-text URL lookup at (column, row) when Ghostty reports no OSC 8 hyperlink.
[[nodiscard]] std::optional<QString> terminalTextLinkAt(
    const TerminalFrame& frame,
    std::uint16_t column,
    std::uint16_t row);

}
