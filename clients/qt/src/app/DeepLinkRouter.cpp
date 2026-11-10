#include "app/DeepLinkRouter.hpp"

#include <QUuid>

namespace kodosi {
namespace {

bool forbiddenCodePoint(const char32_t value)
{
    const auto category = QChar::category(value);
    return category == QChar::Other_Control || category == QChar::Other_Format;
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
    auto session = sessionId(remainder);
    if (!session) {
        return failure(DeepLinkParseError::UnsupportedRoute);
    }

    return {
        .destination = DeepLinkDestination {
            .sessionId = std::move(*session),
        },
    };
}

QString DeepLinkRouter::url(const DeepLinkDestination& destination)
{
    return QStringLiteral("kodosi://session/") + destination.sessionId;
}

std::optional<QString> DeepLinkRouter::sessionId(const QStringView value)
{
    const auto id = QUuid::fromString(value);
    auto text = id.toString(QUuid::WithoutBraces);
    if (id.isNull() || text.compare(value, Qt::CaseInsensitive) != 0) {
        return std::nullopt;
    }
    return text;
}

QString DeepLinkRouter::userMessage(const DeepLinkParseError error)
{
    switch (error) {
    case DeepLinkParseError::Oversized:
        return tr("The link is too long to open safely.");
    case DeepLinkParseError::UnsafeText:
        return tr("The link contains unsafe text.");
    case DeepLinkParseError::InvalidQuery:
        return tr("The link contains unsupported parameters.");
    case DeepLinkParseError::UnsupportedRoute:
    case DeepLinkParseError::None:
        return tr("This Kodosi link is invalid or unsupported.");
    }
    return tr("Kodosi could not open this link.");
}

}
