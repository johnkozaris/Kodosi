#include "app/DeepLinkRouter.hpp"

#include <QByteArray>
#include <QStringDecoder>

namespace kodosi {
namespace {

bool forbiddenCodePoint(const char32_t value)
{
    const auto category = QChar::category(value);
    return value == 0
        || value < 0x20
        || (value >= 0x7f && value <= 0x9f)
        || category == QChar::Other_Control
        || category == QChar::Other_Format
        || value == 0x202a
        || value == 0x202b
        || value == 0x202c
        || value == 0x202d
        || value == 0x202e
        || value == 0x2066
        || value == 0x2067
        || value == 0x2068
        || value == 0x2069
        || value == 0x061c
        || value == 0x200e
        || value == 0x200f;
}

bool hasForbiddenText(const QStringView value)
{
    for (auto index = qsizetype {0}; index < value.size(); ++index) {
        const auto first = value.at(index);
        char32_t codePoint = first.unicode();
        if (first.isHighSurrogate()) {
            if (index + 1 >= value.size()
                || !value.at(index + 1).isLowSurrogate()) {
                return true;
            }
            codePoint = QChar::surrogateToUcs4(
                first,
                value.at(++index));
        } else if (first.isLowSurrogate()) {
            return true;
        }
        if (forbiddenCodePoint(codePoint)) {
            return true;
        }
    }
    return false;
}

DeepLinkParseResult failure(const DeepLinkParseError error)
{
    return {
        .destination = std::nullopt,
        .error = error,
    };
}

bool isHexDigit(const QChar value)
{
    return (value >= u'0' && value <= u'9')
        || (value >= u'a' && value <= u'f')
        || (value >= u'A' && value <= u'F');
}

}

DeepLinkParseResult DeepLinkRouter::parse(const QStringView rawUrl)
{
    if (rawUrl.isEmpty()) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }
    if (rawUrl.size() > MaximumUrlCharacters
        || rawUrl.toUtf8().size() > MaximumUrlCharacters * 4) {
        return failure(DeepLinkParseError::Oversized);
    }
    if (hasForbiddenText(rawUrl)) {
        return failure(DeepLinkParseError::UnsafeText);
    }
    if (rawUrl.contains(u'#')) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }

    const auto schemeSeparator = rawUrl.indexOf(u"://");
    if (schemeSeparator <= 0
        || rawUrl.first(schemeSeparator).compare(
               QStringLiteral("kodosi"),
               Qt::CaseInsensitive)
            != 0) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }
    auto remainder = rawUrl.sliced(schemeSeparator + 3);
    const auto slash = remainder.indexOf(u'/');
    if (slash < 0 || remainder.first(slash) != QStringLiteral("session")) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }
    remainder = remainder.sliced(slash + 1);

    if (remainder.contains(u'?')) {
        return failure(DeepLinkParseError::InvalidQuery);
    }
    const QStringView encodedSession = remainder;
    if (encodedSession.isEmpty() || encodedSession.contains(u'/')) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }

    DeepLinkParseError decodeError = DeepLinkParseError::None;
    auto sessionId = decodeComponent(encodedSession, decodeError);
    if (!sessionId) {
        return failure(decodeError);
    }

    return {
        .destination = DeepLinkDestination {
            .sessionId = std::move(*sessionId),
        },
    };
}

bool DeepLinkRouter::isSafeIdentity(const QStringView value)
{
    return !value.isEmpty()
        && value.size() <= MaximumIdentityCharacters
        && value.toUtf8().size() <= MaximumIdentityCharacters * 4
        && !hasForbiddenText(value);
}

QString DeepLinkRouter::userMessage(const DeepLinkParseError error)
{
    switch (error) {
    case DeepLinkParseError::Oversized:
        return QStringLiteral("The link is too long to open safely.");
    case DeepLinkParseError::MalformedEncoding:
        return QStringLiteral("The link contains malformed text encoding.");
    case DeepLinkParseError::UnsafeText:
        return QStringLiteral("The link contains unsafe text.");
    case DeepLinkParseError::InvalidQuery:
        return QStringLiteral("The link contains unsupported parameters.");
    case DeepLinkParseError::UnsupportedRoute:
    case DeepLinkParseError::None:
        return QStringLiteral("This Kodosi link is invalid or unsupported.");
    }
    return QStringLiteral("This Kodosi link could not be opened.");
}

std::optional<QString> DeepLinkRouter::decodeComponent(
    const QStringView encoded,
    DeepLinkParseError& error)
{
    for (auto index = qsizetype {0}; index < encoded.size(); ++index) {
        if (encoded.at(index) != u'%') {
            continue;
        }
        if (index + 2 >= encoded.size()
            || !isHexDigit(encoded.at(index + 1))
            || !isHexDigit(encoded.at(index + 2))) {
            error = DeepLinkParseError::MalformedEncoding;
            return std::nullopt;
        }
        index += 2;
    }

    const auto bytes = QByteArray::fromPercentEncoding(encoded.toUtf8());
    QStringDecoder decoder(QStringDecoder::Utf8);
    const QString decoded = decoder.decode(bytes);
    if (decoder.hasError()) {
        error = DeepLinkParseError::MalformedEncoding;
        return std::nullopt;
    }
    if (!isSafeIdentity(decoded)
        || decoded.contains(u'/')
        || decoded.contains(u'?')
        || decoded.contains(u'#')) {
        error = DeepLinkParseError::UnsafeText;
        return std::nullopt;
    }
    return decoded;
}

}
