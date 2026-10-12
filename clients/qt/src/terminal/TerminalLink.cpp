#include "terminal/TerminalLink.hpp"

#include <QByteArray>

namespace kodosi {
namespace {

constexpr qsizetype maximumTerminalLinkCharacters = 8'192;

bool containsUnsafeCodepoint(const QString& value)
{
    const auto isBidiControl = [](const char32_t codepoint) {
        return codepoint == 0x061c || codepoint == 0x200e || codepoint == 0x200f
            || (codepoint >= 0x202a && codepoint <= 0x202e)
            || (codepoint >= 0x2066 && codepoint <= 0x2069);
    };
    for (auto iterator = value.constBegin(); iterator != value.constEnd();) {
        char32_t codepoint = iterator->unicode();
        if (iterator->isHighSurrogate()
            && iterator + 1 != value.constEnd()
            && (iterator + 1)->isLowSurrogate()) {
            const auto high = *iterator++;
            const auto low = *iterator++;
            codepoint = QChar::surrogateToUcs4(high, low);
        } else {
            ++iterator;
        }
        if (codepoint <= 0x1f || (codepoint >= 0x7f && codepoint <= 0x9f)
            || isBidiControl(codepoint)) {
            return true;
        }
    }
    return false;
}

}

std::optional<QUrl> validatedTerminalLink(const QString& value)
{
    if (value.isEmpty() || value.size() > maximumTerminalLinkCharacters
        || containsUnsafeCodepoint(value)) {
        return std::nullopt;
    }
    const auto encoded = value.toUtf8();
    const auto decoded = QUrl::fromPercentEncoding(encoded);
    if (containsUnsafeCodepoint(decoded)) {
        return std::nullopt;
    }
    const auto url = QUrl::fromEncoded(encoded, QUrl::StrictMode);
    if (!url.isValid() || url.isRelative()) {
        return std::nullopt;
    }
    const auto scheme = url.scheme().toLower();
    if (scheme != QStringLiteral("http")
        && scheme != QStringLiteral("https")
        && scheme != QStringLiteral("mailto")) {
        return std::nullopt;
    }
    if ((scheme == QStringLiteral("http") || scheme == QStringLiteral("https"))
        && url.host().isEmpty()) {
        return std::nullopt;
    }
    if (scheme == QStringLiteral("mailto") && url.path().isEmpty()) {
        return std::nullopt;
    }
    return url;
}

}
