#include "presentation/ChatProse.hpp"

#include <QVariantMap>

#include <algorithm>

#include <optional>

namespace kodosi {
namespace {
QString escaped(QStringView text)
{
    QString out;
    out.reserve(text.size());
    for (const auto character : text) {
        if (character == u'&')
            out += QStringLiteral("&amp;");
        else if (character == u'<')
            out += QStringLiteral("&lt;");
        else if (character == u'>')
            out += QStringLiteral("&gt;");
        else if (character == u'"')
            out += QStringLiteral("&quot;");
        else
            out += character;
    }
    return out;
}

bool web(QStringView address)
{
    return address.startsWith(u"https://", Qt::CaseInsensitive);
}

bool punctuation(QChar character)
{
    return character.isPunct() || character.isSymbol();
}

// The marks of one line. A mark that is never closed is the agent's own character.
class Inline {
public:
    explicit Inline(QStringView text)
        : m_text(text)
    {
    }

    QString html(qsizetype from, qsizetype to) const
    {
        QString out;
        qsizetype at = from;
        while (at < to) {
            const auto character = m_text[at];
            if (character == u'\\' && at + 1 < to && punctuation(m_text[at + 1])) {
                out += escaped(m_text.sliced(at + 1, 1));
                at += 2;
            } else if (character == u'`') {
                at = code(at, to, out);
            } else if (character == u'[' && link(at, to, out)) {
                continue;
            } else if (address(at, to, out)) {
                continue;
            } else if (emphasis(at, to, out)) {
                continue;
            } else {
                out += escaped(m_text.sliced(at, 1));
                ++at;
            }
        }
        return out;
    }

private:
    qsizetype code(qsizetype at, qsizetype to, QString& out) const
    {
        qsizetype run = 0;
        while (at + run < to && m_text[at + run] == u'`')
            ++run;
        const QString fence(run, u'`');
        auto close = m_text.indexOf(fence, at + run);
        while (close >= 0 && close + run < to && m_text[close + run] == u'`')
            close = m_text.indexOf(fence, close + run + 1);
        if (close < 0 || close + run > to) {
            out += fence;
            return at + run;
        }
        auto inner = m_text.sliced(at + run, close - at - run);
        if (inner.size() > 1 && inner.startsWith(u' ') && inner.endsWith(u' '))
            inner = inner.sliced(1, inner.size() - 2);
        out += QStringLiteral("<code>") + escaped(inner) + QStringLiteral("</code>");
        return close + run;
    }

    bool link(qsizetype& at, qsizetype to, QString& out) const
    {
        const auto middle = m_text.indexOf(u"](", at + 1);
        if (middle < 0 || middle >= to)
            return false;
        const auto close = m_text.indexOf(u')', middle + 2);
        if (close < 0 || close >= to)
            return false;
        const auto target = m_text.sliced(middle + 2, close - middle - 2).trimmed();
        if (!web(target) || target.contains(u' '))
            return false;
        out += QStringLiteral("<a href=\"") + escaped(target) + QStringLiteral("\">") + html(at + 1, middle)
            + QStringLiteral("</a>");
        at = close + 1;
        return true;
    }

    bool address(qsizetype& at, qsizetype to, QString& out) const
    {
        if (!web(m_text.sliced(at, to - at)) || (at > 0 && !m_text[at - 1].isSpace() && m_text[at - 1] != u'('))
            return false;
        auto end = at;
        while (end < to && !m_text[end].isSpace() && m_text[end] != u'<' && m_text[end] != u'>')
            ++end;
        while (end > at && QStringView(u".,;:!?'\")*_").contains(m_text[end - 1]))
            --end;
        const auto target = m_text.sliced(at, end - at);
        if (target.size() <= 8)
            return false;
        out += QStringLiteral("<a href=\"") + escaped(target) + QStringLiteral("\">") + escaped(target)
            + QStringLiteral("</a>");
        at = end;
        return true;
    }

    bool emphasis(qsizetype& at, qsizetype to, QString& out) const
    {
        struct Mark {
            QStringView marker;
            const char* tag;
        };
        static constexpr Mark marks[] { { u"**", "b" }, { u"__", "b" }, { u"~~", "s" }, { u"*", "i" }, { u"_", "i" } };
        for (const auto& mark : marks) {
            const auto size = mark.marker.size();
            if (!m_text.sliced(at, to - at).startsWith(mark.marker) || at + size >= to || m_text[at + size].isSpace())
                continue;
            const bool word = mark.marker.startsWith(u'_');
            if (word && at > 0 && m_text[at - 1].isLetterOrNumber())
                continue;
            auto close = m_text.indexOf(mark.marker, at + size + 1);
            while (close >= 0 && close < to) {
                const bool spaced = m_text[close - 1].isSpace();
                const bool inWord = word && close + size < to && m_text[close + size].isLetterOrNumber();
                const bool longer = size == 1 && close + 1 < to && m_text[close + 1] == mark.marker[0];
                if (!spaced && !inWord && !longer)
                    break;
                close = m_text.indexOf(mark.marker, close + (longer ? 2 : 1));
            }
            if (close < 0 || close + size > to)
                continue;
            const auto tag = QString::fromLatin1(mark.tag);
            out += u'<' + tag + u'>' + html(at + size, close) + QStringLiteral("</") + tag + u'>';
            at = close + size;
            return true;
        }
        return false;
    }

    QStringView m_text;
};

std::optional<QStringView> heading(QStringView line)
{
    qsizetype marks = 0;
    while (marks < line.size() && line[marks] == u'#')
        ++marks;
    if (marks < 1 || marks > 6 || marks >= line.size() || line[marks] != u' ')
        return std::nullopt;
    return line.sliced(marks + 1);
}

QString bulleted(QStringView line)
{
    qsizetype indent = 0;
    while (indent < line.size() && line[indent] == u' ')
        ++indent;
    const auto rest = line.sliced(indent);
    if (!rest.startsWith(u"- ") && !rest.startsWith(u"* ") && !rest.startsWith(u"+ "))
        return line.toString();
    return line.first(indent).toString() + QStringLiteral("• ") + rest.sliced(2).toString();
}

std::optional<QStringList> sides(const QStringList& row)
{
    if (row.isEmpty())
        return std::nullopt;
    QStringList sides;
    for (const auto& cell : row) {
        if (!cell.contains(u'-') || std::ranges::any_of(cell, [](QChar c) { return c != u'-' && c != u':' && c != u' '; }))
            return std::nullopt;
        const bool left = cell.startsWith(u':');
        const bool right = cell.endsWith(u':');
        sides.append(left && right ? QStringLiteral("center") : right ? QStringLiteral("right") : QStringLiteral("left"));
    }
    return sides;
}

QVariantMap table(const QStringList& lines)
{
    QList<QStringList> rows;
    for (const auto& line : lines)
        rows.append(ChatProse::cells(line));
    QStringList header;
    QStringList aligned;
    if (rows.size() > 1) {
        if (const auto found = sides(rows.at(1))) {
            header = rows.takeFirst();
            rows.removeFirst();
            aligned = *found;
        }
    }
    qsizetype columns = header.size();
    for (const auto& row : rows)
        columns = std::max(columns, row.size());
    const auto shaped = [columns](const QStringList& row) {
        QVariantList cells;
        for (qsizetype column = 0; column < columns; ++column)
            cells.append(column < row.size() ? ChatProse::html(row.at(column)) : QString {});
        return cells;
    };
    QVariantList shapedRows;
    for (const auto& row : rows)
        shapedRows.append(QVariant(shaped(row)));
    QVariantList shapedSides;
    for (qsizetype column = 0; column < columns; ++column)
        shapedSides.append(column < aligned.size() ? aligned.at(column) : QStringLiteral("left"));
    return { { QStringLiteral("kind"), QStringLiteral("table") },
        { QStringLiteral("header"), header.isEmpty() ? QVariantList {} : shaped(header) },
        { QStringLiteral("rows"), shapedRows }, { QStringLiteral("sides"), shapedSides },
        { QStringLiteral("columns"), columns } };
}
}

QString ChatProse::html(QStringView text)
{
    return Inline(text).html(0, text.size());
}

QString ChatProse::text(const QString& text)
{
    QStringList out;
    for (const auto line : QStringView(text).split(u'\n'))
        out.append(html(line));
    return QStringLiteral("<span style=\"white-space:pre-wrap\">") + out.join(u'\n') + QStringLiteral("</span>");
}

QStringList ChatProse::cells(QStringView line)
{
    QStringList cells;
    QString cell;
    bool escape = false;
    for (const auto character : line) {
        if (escape) {
            if (character != u'|')
                cell += u'\\';
            cell += character;
            escape = false;
        } else if (character == u'\\') {
            escape = true;
        } else if (character == u'|') {
            cells.append(cell);
            cell.clear();
        } else {
            cell += character;
        }
    }
    if (escape)
        cell += u'\\';
    cells.append(cell);
    for (auto& each : cells)
        each = each.trimmed();
    if (!cells.isEmpty() && cells.first().isEmpty())
        cells.removeFirst();
    if (!cells.isEmpty() && cells.last().isEmpty())
        cells.removeLast();
    return cells;
}

QVariantList ChatProse::blocks(const QString& source)
{
    const QStringView text(source);
    QVariantList blocks;
    QStringList lines;
    QStringList rows;
    std::optional<QStringList> code;
    const auto flush = [&] {
        if (!rows.isEmpty()) {
            blocks.append(table(rows));
            rows.clear();
        }
        auto joined = lines.join(u'\n');
        qsizetype start = 0;
        while (start < joined.size() && joined[start] == u'\n')
            ++start;
        auto end = joined.size();
        while (end > start && joined[end - 1] == u'\n')
            --end;
        joined = joined.sliced(start, end - start);
        if (!joined.isEmpty())
            blocks.append(QVariantMap { { QStringLiteral("kind"), QStringLiteral("lines") }, { QStringLiteral("html"), ChatProse::text(joined) } });
        lines.clear();
    };
    for (const auto line : text.split(u'\n')) {
        qsizetype indent = 0;
        while (indent < line.size() && line[indent] == u' ')
            ++indent;
        const auto trimmed = line.sliced(indent);
        if (trimmed.startsWith(u"```") || trimmed.startsWith(u"~~~")) {
            if (code) {
                blocks.append(QVariantMap { { QStringLiteral("kind"), QStringLiteral("code") }, { QStringLiteral("text"), code->join(u'\n') } });
                code.reset();
            } else {
                flush();
                code = QStringList {};
            }
        } else if (code) {
            code->append(line.toString());
        } else if (trimmed.startsWith(u'|')) {
            if (rows.isEmpty())
                flush();
            rows.append(line.toString());
        } else if (const auto title = heading(trimmed)) {
            flush();
            blocks.append(QVariantMap { { QStringLiteral("kind"), QStringLiteral("heading") }, { QStringLiteral("html"), ChatProse::html(*title) } });
        } else {
            if (!rows.isEmpty())
                flush();
            lines.append(bulleted(line));
        }
    }
    if (code)
        blocks.append(QVariantMap { { QStringLiteral("kind"), QStringLiteral("code") }, { QStringLiteral("text"), code->join(u'\n') } });
    flush();
    for (qsizetype place = 0; place < blocks.size(); ++place) {
        auto block = blocks.at(place).toMap();
        block.insert(QStringLiteral("key"), QString::number(place));
        blocks[place] = block;
    }
    return blocks;
}

}
