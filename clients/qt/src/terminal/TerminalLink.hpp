#pragma once

#include <QString>
#include <QUrl>

#include <cstdint>
#include <optional>

namespace kodosi {

struct TerminalFrame;

// A link that a person clicked opens by Ghostty's rules: web and mail links open, a
// file or folder opens unless it can run code, a link to another app asks first, and
// a malformed or disguised link does not open. A file or a local address in a terminal
// of another computer is on that computer, so it does not open here.
struct TerminalLinkDecision {
    enum class Action {
        Open,
        Ask,
        Refuse,
    };

    Action action;
    QUrl url;
    QString shown;
    QString reason;
};

// `otherComputer` names the computer that runs the terminal when it is not this one.
[[nodiscard]] TerminalLinkDecision decideTerminalLink(
    const QString& value,
    const QString& otherComputer = {});
[[nodiscard]] std::optional<QString> terminalTextLinkAt(
    const TerminalFrame& frame,
    std::uint16_t column,
    std::uint16_t row);

}
