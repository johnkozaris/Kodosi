#include "terminal/TerminalLink.hpp"

#include "presentation/Link.hpp"
#include "terminal/TerminalFrame.hpp"

#include <QRegularExpression>
#include <QStringView>
#include <QUrl>

namespace kodosi {

std::optional<QString> terminalTextLinkAt(
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
        auto value = text.sliced(start, end - start);
        const QUrl url(value, QUrl::StrictMode);
        if (value.size() > maximumLinkCharacters || !url.isValid() || url.host().isEmpty()) {
            return std::nullopt;
        }
        return value;
    }
    return std::nullopt;
}

}
