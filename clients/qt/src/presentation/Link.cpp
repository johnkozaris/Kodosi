#include "presentation/Link.hpp"

#include <kodosi_runtime.h>

#include <QByteArray>
#include <QCoreApplication>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeDatabase>
#include <QSet>
#include <QStringList>

#include <algorithm>

namespace kodosi {
namespace {

// Refuse local files that the OS could execute rather than open as data.
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

QString linkText(const char* text)
{
    return QCoreApplication::translate("Link", text);
}

QString reason(const QString& refusal, const QString& place)
{
    if (refusal == QStringLiteral("hiddenCharacters")) {
        return linkText(QT_TRANSLATE_NOOP("Link", "The link has hidden characters that can change how it looks."));
    }
    if (refusal == QStringLiteral("missingFile")) {
        return linkText(QT_TRANSLATE_NOOP("Link", "The file or folder is not on this device."));
    }
    if (refusal == QStringLiteral("program")) {
        return linkText(QT_TRANSLATE_NOOP("Link", "Opening it could run a program."));
    }
    if (refusal == QStringLiteral("fileElsewhere")) {
        return linkText(QT_TRANSLATE_NOOP("Link", "The file is on %1.")).arg(place);
    }
    if (refusal == QStringLiteral("addressElsewhere")) {
        return linkText(QT_TRANSLATE_NOOP("Link", "The address is on %1.")).arg(place);
    }
    return linkText(QT_TRANSLATE_NOOP("Link", "The link is not a complete address."));
}

QJsonObject runtimeDecision(const QString& value, const QString& otherComputer)
{
    const auto link = value.toUtf8();
    const auto computer = otherComputer.toUtf8();
    QByteArray decision(4096, Qt::Uninitialized);
    const auto decide = [&] {
        return kodosi_link_decision(
            reinterpret_cast<const uint8_t*>(link.constData()), static_cast<uintptr_t>(link.size()),
            reinterpret_cast<const uint8_t*>(computer.constData()), static_cast<uintptr_t>(computer.size()),
            reinterpret_cast<uint8_t*>(decision.data()), static_cast<uintptr_t>(decision.size()));
    };
    auto length = decide();
    if (length > decision.size()) {
        decision.resize(static_cast<qsizetype>(length));
        length = decide();
    }
    if (length < 0 || length > decision.size()) {
        return {};
    }
    return QJsonDocument::fromJson(decision.first(static_cast<qsizetype>(length))).object();
}

}

LinkDecision decideLink(const QString& value, const QString& otherComputer)
{
    const auto decided = runtimeDecision(value, otherComputer);
    const auto shown = decided.value(QStringLiteral("shown")).toString(value.left(maximumLinkCharacters));
    const auto action = decided.value(QStringLiteral("action")).toString();
    const QUrl target(decided.value(QStringLiteral("target")).toString(), QUrl::StrictMode);
    if ((action == QStringLiteral("open") || action == QStringLiteral("ask")) && target.isValid()) {
        if (action == QStringLiteral("ask")) {
            return {LinkDecision::Action::Ask, target, shown, {}};
        }
        if (target.isLocalFile() && runsCode(QFileInfo(target.toLocalFile()))) {
            return {LinkDecision::Action::Refuse, QUrl(), shown, reason(QStringLiteral("program"), {})};
        }
        return {LinkDecision::Action::Open, target, shown, {}};
    }
    return {
        LinkDecision::Action::Refuse,
        QUrl(),
        shown,
        reason(decided.value(QStringLiteral("refusal")).toString(), decided.value(QStringLiteral("place")).toString()),
    };
}

}
