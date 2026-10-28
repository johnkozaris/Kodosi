#include "terminal/TerminalLink.hpp"
#include "terminal/TerminalFrame.hpp"

#include <QByteArray>
#include <QCoreApplication>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QRegularExpression>
#include <QSet>
#include <QStringView>
#include <QSysInfo>

#include <algorithm>

namespace kodosi {
namespace {

constexpr qsizetype maximumTerminalLinkCharacters = 8'192;

// Controls, line breaks, direction marks and invisible marks can make a link look
// like another one.
bool isHidden(const char32_t codepoint)
{
    return codepoint <= 0x1f || (codepoint >= 0x7f && codepoint <= 0x9f) || codepoint == 0x061c
        || (codepoint >= 0x200b && codepoint <= 0x200f) || (codepoint >= 0x202a && codepoint <= 0x202e)
        || (codepoint >= 0x2028 && codepoint <= 0x2029) || codepoint == 0x2060
        || (codepoint >= 0x2066 && codepoint <= 0x2069) || codepoint == 0xfeff;
}

bool containsHidden(const QString& value)
{
    return std::ranges::any_of(value.toUcs4(), [](const char32_t codepoint) { return isHidden(codepoint); });
}

// The link on one line, with its hidden characters written out.
QString shownText(const QString& value)
{
    QString shown;
    for (const char32_t codepoint : value.left(maximumTerminalLinkCharacters).toUcs4()) {
        shown += isHidden(codepoint)
            ? QStringLiteral("\\u{%1}").arg(QString::number(static_cast<uint>(codepoint), 16).toUpper())
            : QString::fromUcs4(&codepoint, 1);
    }
    return shown;
}

QString linkText(const char* text)
{
    return QCoreApplication::translate("TerminalLink", text);
}

TerminalLinkDecision refuse(QString shown, const char* reason, const QString& place = {})
{
    auto text = linkText(reason);
    return {
        TerminalLinkDecision::Action::Refuse,
        QUrl(),
        std::move(shown),
        place.isEmpty() ? text : text.arg(place),
    };
}

// Programs such as `ls --hyperlink` name the computer in a file link.
bool isThisComputer(const QString& host)
{
    return host.compare(QStringLiteral("localhost"), Qt::CaseInsensitive) == 0
        || host.compare(QSysInfo::machineHostName(), Qt::CaseInsensitive) == 0;
}

bool isByte(const QString& part)
{
    bool number = false;
    const auto value = part.toUInt(&number);
    return number && value <= 255;
}

// An address on the loopback interface names the computer that serves it.
bool isLoopback(QString host)
{
    host = host.toLower();
    if (host.startsWith(u'[') && host.endsWith(u']')) {
        host = host.sliced(1, host.size() - 2);
    }
    if (host == QStringLiteral("localhost") || host.endsWith(QStringLiteral(".localhost"))
        || host == QStringLiteral("0.0.0.0") || host == QStringLiteral("::") || host == QStringLiteral("::1")) {
        return true;
    }
    const auto parts = host.split(u'.');
    return parts.size() == 4 && parts.first() == QStringLiteral("127") && std::ranges::all_of(parts, isByte);
}

// Opening one of these runs code, whatever the program in the terminal says it is.
bool runsCode(const QFileInfo& target)
{
    static const QSet<QString> suffixes {
        // Linux
        QStringLiteral("appimage"), QStringLiteral("bash"), QStringLiteral("csh"),
        QStringLiteral("deb"), QStringLiteral("desktop"), QStringLiteral("fish"),
        QStringLiteral("flatpakref"), QStringLiteral("jar"), QStringLiteral("ksh"),
        QStringLiteral("pl"), QStringLiteral("py"), QStringLiteral("rb"),
        QStringLiteral("rpm"), QStringLiteral("run"), QStringLiteral("sh"),
        QStringLiteral("snap"), QStringLiteral("zsh"),
        // Windows
        QStringLiteral("application"), QStringLiteral("appref-ms"), QStringLiteral("bat"),
        QStringLiteral("chm"), QStringLiteral("cmd"), QStringLiteral("com"),
        QStringLiteral("cpl"), QStringLiteral("exe"), QStringLiteral("hta"),
        QStringLiteral("inf"), QStringLiteral("js"), QStringLiteral("jse"),
        QStringLiteral("lnk"), QStringLiteral("msc"), QStringLiteral("msi"),
        QStringLiteral("msp"), QStringLiteral("pif"), QStringLiteral("ps1"),
        QStringLiteral("psm1"), QStringLiteral("reg"), QStringLiteral("scf"),
        QStringLiteral("scr"), QStringLiteral("url"), QStringLiteral("vbe"),
        QStringLiteral("vbs"), QStringLiteral("wsf"), QStringLiteral("wsh"),
    };
    static const QStringList types {
        QStringLiteral("application/x-executable"),
        QStringLiteral("application/x-pie-executable"),
        QStringLiteral("application/x-sharedlib"),
        QStringLiteral("application/x-shellscript"),
        QStringLiteral("application/x-desktop"),
        QStringLiteral("application/x-ms-dos-executable"),
        QStringLiteral("application/x-msdownload"),
        QStringLiteral("application/x-msi"),
        QStringLiteral("application/vnd.appimage"),
        QStringLiteral("application/x-java-archive"),
    };
    if (suffixes.contains(target.suffix().toLower())) {
        return true;
    }
    if (target.isDir()) {
        return false;
    }
#ifndef Q_OS_WIN
    if (target.isExecutable()) {
        return true;
    }
#endif
    const auto type = QMimeDatabase().mimeTypeForFile(target);
    return std::ranges::any_of(types, [&](const QString& name) { return type.inherits(name); });
}

TerminalLinkDecision fileDecision(const QUrl& link, const QString& shown, const QString& otherComputer)
{
    if (link.hasQuery() || link.hasFragment()) {
        return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The link is not a complete address."));
    }
    auto local = link;
    local.setHost(QString());
    if (!otherComputer.isEmpty()) {
        return refuse(local.toLocalFile(), QT_TRANSLATE_NOOP("TerminalLink", "The file is on %1."), otherComputer);
    }
    if (const auto host = link.host(); !host.isEmpty() && !isThisComputer(host)) {
        return refuse(local.toLocalFile(), QT_TRANSLATE_NOOP("TerminalLink", "The file is on %1."), host);
    }
    const QFileInfo named(local.toLocalFile());
    const auto canonical = named.canonicalFilePath();
    const QFileInfo target(canonical);
    if (canonical.isEmpty() || (!target.isDir() && !target.isFile())) {
        return refuse(
            named.filePath(),
            QT_TRANSLATE_NOOP("TerminalLink", "The file or folder is not on this device."));
    }
    if (runsCode(target)) {
        return refuse(canonical, QT_TRANSLATE_NOOP("TerminalLink", "Opening it could run a program."));
    }
    return {TerminalLinkDecision::Action::Open, QUrl::fromLocalFile(canonical), canonical, {}};
}

}

TerminalLinkDecision decideTerminalLink(const QString& value, const QString& otherComputer)
{
    const auto shown = shownText(value);
    const auto encoded = value.toUtf8();
    if (containsHidden(value) || containsHidden(QUrl::fromPercentEncoding(encoded))) {
        return refuse(
            shown,
            QT_TRANSLATE_NOOP("TerminalLink", "The link has hidden characters that can change how it looks."));
    }
    const auto link = QUrl::fromEncoded(encoded, QUrl::StrictMode);
    if (!otherComputer.isEmpty() && link.isValid() && link.isRelative() && !value.isEmpty()) {
        return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The file is on %1."), otherComputer);
    }
    if (value.size() > maximumTerminalLinkCharacters || !link.isValid() || link.isRelative()) {
        return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The link is not a complete address."));
    }
    const auto scheme = link.scheme().toLower();
    if (scheme == QStringLiteral("http") || scheme == QStringLiteral("https")) {
        if (link.host().isEmpty()) {
            return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The web link has no host."));
        }
        if (!otherComputer.isEmpty() && isLoopback(link.host())) {
            return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The address is on %1."), otherComputer);
        }
        return {TerminalLinkDecision::Action::Open, link, shown, {}};
    }
    if (scheme == QStringLiteral("mailto")) {
        if (link.path().isEmpty()) {
            return refuse(shown, QT_TRANSLATE_NOOP("TerminalLink", "The link is not a complete address."));
        }
        return {TerminalLinkDecision::Action::Open, link, shown, {}};
    }
    if (scheme == QStringLiteral("file")) {
        return fileDecision(link, shown, otherComputer);
    }
    return {TerminalLinkDecision::Action::Ask, link, shown, {}};
}

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
        if (value.size() > maximumTerminalLinkCharacters || !url.isValid() || url.host().isEmpty()) {
            return std::nullopt;
        }
        return value;
    }
    return std::nullopt;
}

}
