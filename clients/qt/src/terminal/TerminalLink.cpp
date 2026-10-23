#include "terminal/TerminalLink.hpp"
#include "terminal/TerminalFrame.hpp"

#include <QByteArray>
#include <QRegularExpression>
#include <QStringView>

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
    return url;
}

std::optional<QUrl> terminalTextLinkAt(
    const TerminalFrame& frame,
    const std::uint16_t column,
    const std::uint16_t row)
{
    if (column >= frame.columns || row >= frame.rows || frame.wrappedRows.size() != frame.rows) {
        return std::nullopt;
    }
    int first = row;
    while (first > 0 && frame.wrappedRows[first - 1]) {
        --first;
    }
    int last = row;
    while (last + 1 < frame.rows && frame.wrappedRows[last]) {
        ++last;
    }
    QString text;
    qsizetype target = -1;
    for (int y = first; y <= last; ++y) {
        for (int x = 0; x < frame.columns; ++x) {
            const auto* cell = frame.cell(static_cast<std::uint16_t>(x), static_cast<std::uint16_t>(y));
            if (cell == nullptr) {
                return std::nullopt;
            }
            const bool spacer = cell->width == 0;
            if (y == row && x == column) {
                target = spacer ? text.size() - 1 : text.size();
            }
            if (!spacer) {
                text += cell->grapheme.isEmpty() ? QStringLiteral(" ") : cell->grapheme;
            }
        }
    }
    static const QRegularExpression address(
        QStringLiteral(R"(https?://[^\s<>"'`{}|\\^\x{2500}-\x{259F}]+)"),
        QRegularExpression::CaseInsensitiveOption);
    for (auto matches = address.globalMatch(text); matches.hasNext();) {
        const auto match = matches.next();
        const auto start = match.capturedStart();
        auto end = match.capturedEnd();
        const auto unbalanced = [&](const QChar open, const QChar close) {
            const auto span = QStringView(text).sliced(start, end - start);
            return span.count(close) > span.count(open);
        };
        while (end > start) {
            const auto ending = text.at(end - 1);
            if (QStringView(u".,;:!?").contains(ending)
                || (ending == u')' && unbalanced(u'(', u')'))
                || (ending == u']' && unbalanced(u'[', u']'))) {
                --end;
            } else {
                break;
            }
        }
        if (target < start || target >= end) {
            continue;
        }
        auto url = validatedTerminalLink(text.sliced(start, end - start));
        return url && !url->host().isEmpty() ? url : std::nullopt;
    }
    return std::nullopt;
}

}
