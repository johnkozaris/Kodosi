#include "AppLanguage.hpp"

#include <QCoreApplication>
#include <QLibraryInfo>
#include <QLocale>
#include <QTranslator>
#include <utility>

namespace kodosi::AppLanguage {
namespace {

QString g_current = QStringLiteral("en");
QTranslator* g_own = nullptr;
QTranslator* g_qt = nullptr;

// Qt names Simplified Chinese by its region.
QString qtName(const QString& tag)
{
    return tag == QStringLiteral("zh-Hans") ? QStringLiteral("zh_CN") : tag;
}

QString supported(const QString& asked)
{
    const auto lower = asked.toLower().replace(QLatin1Char('_'), QLatin1Char('-'));
    if (lower.startsWith(QStringLiteral("zh")))
        return !lower.contains(QStringLiteral("hans"))
                && (lower.contains(QStringLiteral("hant")) || lower.endsWith(QStringLiteral("-tw"))
                    || lower.endsWith(QStringLiteral("-hk")) || lower.endsWith(QStringLiteral("-mo")))
            ? QString()
            : QStringLiteral("zh-Hans");
    for (const auto& choice : choices()) {
        if (lower == choice.tag || lower.startsWith(choice.tag + QLatin1Char('-')))
            return choice.tag;
    }
    return {};
}

QString system()
{
    for (const auto& asked : QLocale::system().uiLanguages()) {
        if (const auto tag = supported(asked); !tag.isEmpty())
            return tag;
    }
    return QStringLiteral("en");
}

}

const QList<Choice>& choices()
{
    static const QList<Choice> all {
        { QStringLiteral("en"), QStringLiteral("English") },
        { QStringLiteral("de"), QStringLiteral("Deutsch") },
        { QStringLiteral("el"), QStringLiteral("Ελληνικά") },
        { QStringLiteral("es"), QStringLiteral("Español") },
        { QStringLiteral("fr"), QStringLiteral("Français") },
        { QStringLiteral("it"), QStringLiteral("Italiano") },
        { QStringLiteral("ja"), QStringLiteral("日本語") },
        { QStringLiteral("ko"), QStringLiteral("한국어") },
        { QStringLiteral("zh-Hans"), QStringLiteral("简体中文") },
    };
    return all;
}

QString resolved(const QString& chosen)
{
    const auto tag = supported(chosen);
    return tag.isEmpty() ? system() : tag;
}

QString name(const QString& tag)
{
    for (const auto& choice : choices()) {
        if (choice.tag == tag)
            return choice.name;
    }
    return tag;
}

void install(QCoreApplication& app, const QString& tag)
{
    for (auto* translator : { std::exchange(g_own, nullptr), std::exchange(g_qt, nullptr) }) {
        if (translator != nullptr) {
            QCoreApplication::removeTranslator(translator);
            delete translator;
        }
    }
    g_current = tag;
    const QLocale locale(qtName(tag));
    QLocale::setDefault(locale);
    g_own = new QTranslator(&app);
    if (g_own->load(QStringLiteral(":/i18n/kodosi_%1.qm").arg(qtName(tag))))
        QCoreApplication::installTranslator(g_own);
    if (tag == QStringLiteral("en"))
        return;
    g_qt = new QTranslator(&app);
    if (g_qt->load(locale, QStringLiteral("qt"), QStringLiteral("_"), QLibraryInfo::path(QLibraryInfo::TranslationsPath)))
        QCoreApplication::installTranslator(g_qt);
}

QString current()
{
    return g_current;
}

}
