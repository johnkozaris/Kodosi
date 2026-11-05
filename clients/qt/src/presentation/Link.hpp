#pragma once

#include <QString>
#include <QUrl>

namespace kodosi {

inline constexpr qsizetype maximumLinkCharacters = 8'192;

// A link that a person clicked in a terminal, or on a page or in a file of a canvas. Its text
// can come from anyone, so it opens by Ghostty's rules: web and mail links open in the browser of
// this device, a file or folder opens unless it can run code, a link to another app asks first,
// and a malformed or disguised link does not open. A file or a local address of a terminal on
// another computer is on that computer, so it does not open here. Only the shared browser opens
// links in Kodosi, when a person clicks in it or an agent asks.
struct LinkDecision {
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

// `otherComputer` names the computer of the terminal when it is not this one.
[[nodiscard]] LinkDecision decideLink(const QString& value, const QString& otherComputer = {});

}
