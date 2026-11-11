#pragma once

#include <QString>
#include <QUrl>

namespace kodosi {

inline constexpr qsizetype maximumLinkCharacters = 8'192;

// The text of a link can come from anyone, so it opens by Ghostty's rules: web and mail links
// open in the browser of this device, a file or folder opens unless it can run code, a link to
// another app asks first, and a malformed or disguised link does not open. A file or a local
// address of a terminal on another computer is on that computer, so it does not open here.
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

[[nodiscard]] LinkDecision decideLink(const QString& value, const QString& otherComputer = {});

}
