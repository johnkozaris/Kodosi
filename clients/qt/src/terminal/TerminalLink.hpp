#pragma once

#include <QString>

#include <cstdint>
#include <optional>

namespace kodosi {

struct TerminalFrame;

// The web address under a cell of the terminal's text, when Ghostty did not find one.
[[nodiscard]] std::optional<QString> terminalTextLinkAt(
    const TerminalFrame& frame,
    std::uint16_t column,
    std::uint16_t row);

}
