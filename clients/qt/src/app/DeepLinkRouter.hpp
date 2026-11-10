#pragma once

#include <QCoreApplication>
#include <QString>
#include <QStringView>

#include <optional>

namespace kodosi {

struct DeepLinkDestination {
    QString sessionId;

    bool operator==(const DeepLinkDestination&) const = default;
};

enum class DeepLinkParseError {
    None,
    Oversized,
    UnsupportedRoute,
    UnsafeText,
    InvalidQuery,
};

struct DeepLinkParseResult {
    std::optional<DeepLinkDestination> destination;
    DeepLinkParseError error = DeepLinkParseError::None;
};

class DeepLinkRouter final {
    Q_DECLARE_TR_FUNCTIONS(DeepLinkRouter)

public:
    static constexpr qsizetype MaximumUrlCharacters = 4096;

    [[nodiscard]] static DeepLinkParseResult parse(QStringView rawUrl);
    [[nodiscard]] static QString url(const DeepLinkDestination& destination);
    // The lowercase ID of a terminal, for a UUID in any letter case.
    [[nodiscard]] static std::optional<QString> sessionId(QStringView value);
    [[nodiscard]] static QString userMessage(DeepLinkParseError error);
};

}
