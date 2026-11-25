#pragma once

#include <QString>
#include <QUrl>

namespace kodosi {

inline constexpr qsizetype maximumLinkCharacters = 8'192;

// The runtime decides each link with the same rules for every Kodosi app. Whether opening a local
// file would run a program is checked here, because the desktop decides that.
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
