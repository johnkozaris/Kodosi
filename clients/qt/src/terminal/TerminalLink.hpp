#pragma once

#include <QString>
#include <QUrl>

#include <cstdint>
#include <optional>

namespace kodosi {

struct TerminalFrame;

[[nodiscard]] std::optional<QUrl> validatedTerminalLink(const QString& value);
[[nodiscard]] std::optional<QUrl> terminalTextLinkAt(
    const TerminalFrame& frame,
    std::uint16_t column,
    std::uint16_t row);

}
