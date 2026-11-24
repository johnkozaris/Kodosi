#pragma once

#include <QString>
#include <QUrl>

namespace kodosi {

inline constexpr qsizetype maximumLinkCharacters = 8'192;

// The runtime decides a link by the rule of every app. Whether opening a file of this computer
// runs a program is this app's part, because the desktop decides that.
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
