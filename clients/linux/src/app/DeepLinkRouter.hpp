#pragma once

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
    MalformedEncoding,
    UnsafeText,
    InvalidQuery,
};

struct DeepLinkParseResult {
    std::optional<DeepLinkDestination> destination;
    DeepLinkParseError error = DeepLinkParseError::None;

    [[nodiscard]] bool accepted() const noexcept
    {
        return destination.has_value();
    }
};

class DeepLinkRouter final {
public:
    static constexpr qsizetype MaximumUrlCharacters = 4096;
    static constexpr qsizetype MaximumIdentityCharacters = 512;

    [[nodiscard]] static DeepLinkParseResult parse(QStringView rawUrl);
    [[nodiscard]] static bool isSafeIdentity(QStringView value);
    [[nodiscard]] static QString userMessage(DeepLinkParseError error);

private:
    [[nodiscard]] static std::optional<QString> decodeComponent(
        QStringView encoded,
        DeepLinkParseError& error);
};

}
