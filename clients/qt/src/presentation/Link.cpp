#include "presentation/Link.hpp"

#include <QByteArray>
#include <QCoreApplication>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QSet>
#include <QStringList>
#include <QSysInfo>

#include <algorithm>

namespace kodosi {
namespace {

bool isByte(const QString& part)
{
    bool number = false;
    const auto value = part.toUInt(&number);
    return number && value <= 255;
}

// An address on the loopback interface names the computer that serves it.
bool isLocalAddress(QString host)
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

QString shownText(const QString& value)
{
    QString shown;
    for (const char32_t codepoint : value.left(maximumLinkCharacters).toUcs4()) {
        shown += isHidden(codepoint)
            ? QStringLiteral("\\u{%1}").arg(QString::number(static_cast<uint>(codepoint), 16).toUpper())
            : QString::fromUcs4(&codepoint, 1);
    }
    return shown;
}

QString linkText(const char* text)
{
    return QCoreApplication::translate("Link", text);
}

LinkDecision refuse(QString shown, const char* reason, const QString& place = {})
{
    auto text = linkText(reason);
    return {
        LinkDecision::Action::Refuse,
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

LinkDecision fileDecision(const QUrl& link, const QString& shown, const QString& otherComputer)
{
    if (link.hasQuery() || link.hasFragment()) {
        return refuse(shown, QT_TRANSLATE_NOOP("Link", "The link is not a complete address."));
    }
    auto local = link;
    local.setHost(QString());
    if (!otherComputer.isEmpty()) {
        return refuse(local.toLocalFile(), QT_TRANSLATE_NOOP("Link", "The file is on %1."), otherComputer);
    }
    if (const auto host = link.host(); !host.isEmpty() && !isThisComputer(host)) {
        return refuse(local.toLocalFile(), QT_TRANSLATE_NOOP("Link", "The file is on %1."), host);
    }
    const QFileInfo named(local.toLocalFile());
    const auto canonical = named.canonicalFilePath();
    const QFileInfo target(canonical);
    if (canonical.isEmpty() || (!target.isDir() && !target.isFile())) {
        return refuse(
            named.filePath(),
            QT_TRANSLATE_NOOP("Link", "The file or folder is not on this device."));
    }
    if (runsCode(target)) {
        return refuse(canonical, QT_TRANSLATE_NOOP("Link", "Opening it could run a program."));
    }
    return {LinkDecision::Action::Open, QUrl::fromLocalFile(canonical), canonical, {}};
}

}

LinkDecision decideLink(const QString& value, const QString& otherComputer)
{
    const auto shown = shownText(value);
    const auto encoded = value.toUtf8();
    if (containsHidden(value) || containsHidden(QUrl::fromPercentEncoding(encoded))) {
        return refuse(
            shown,
            QT_TRANSLATE_NOOP("Link", "The link has hidden characters that can change how it looks."));
    }
    const auto link = QUrl::fromEncoded(encoded, QUrl::StrictMode);
    if (!otherComputer.isEmpty() && link.isValid() && link.isRelative() && !value.isEmpty()) {
        return refuse(shown, QT_TRANSLATE_NOOP("Link", "The file is on %1."), otherComputer);
    }
    if (value.size() > maximumLinkCharacters || !link.isValid() || link.isRelative()) {
        return refuse(shown, QT_TRANSLATE_NOOP("Link", "The link is not a complete address."));
    }
    const auto scheme = link.scheme().toLower();
    if (scheme == QStringLiteral("http") || scheme == QStringLiteral("https")) {
        if (link.host().isEmpty()) {
            return refuse(shown, QT_TRANSLATE_NOOP("Link", "The web link has no server name."));
        }
        if (!otherComputer.isEmpty() && isLocalAddress(link.host())) {
            return refuse(shown, QT_TRANSLATE_NOOP("Link", "The address is on %1."), otherComputer);
        }
        return {LinkDecision::Action::Open, link, shown, {}};
    }
    if (scheme == QStringLiteral("mailto")) {
        if (link.path().isEmpty()) {
            return refuse(shown, QT_TRANSLATE_NOOP("Link", "The link is not a complete address."));
        }
        return {LinkDecision::Action::Open, link, shown, {}};
    }
    if (scheme == QStringLiteral("file")) {
        return fileDecision(link, shown, otherComputer);
    }
    return {LinkDecision::Action::Ask, link, shown, {}};
}

}
