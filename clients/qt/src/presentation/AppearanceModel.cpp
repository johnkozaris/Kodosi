#include "presentation/AppearanceModel.hpp"

#include <QAccessibilityHints>
#include <QGuiApplication>
#include <QStyleHints>

#include <optional>
#include <utility>

namespace kodosi {
namespace {

const auto preferenceKey = QStringLiteral("appearance/preference");

QString preferenceName(const AppearanceModel::Preference preference)
{
    switch (preference) {
    case AppearanceModel::Preference::System:
        return QStringLiteral("system");
    case AppearanceModel::Preference::Light:
        return QStringLiteral("light");
    case AppearanceModel::Preference::Dark:
        return QStringLiteral("dark");
    }
    return {};
}

std::optional<AppearanceModel::Preference> preferenceFromName(const QString& name)
{
    if (name == QStringLiteral("system"))
        return AppearanceModel::Preference::System;
    if (name == QStringLiteral("light"))
        return AppearanceModel::Preference::Light;
    if (name == QStringLiteral("dark"))
        return AppearanceModel::Preference::Dark;
    return std::nullopt;
}

AppearanceModel::StyleHooks applicationStyleHooks()
{
    auto* hints = QGuiApplication::styleHints();
    Q_ASSERT(hints != nullptr);
    return {
        .colorScheme = [hints] { return hints->colorScheme(); },
        .setColorScheme =
            [hints](const Qt::ColorScheme scheme) {
                hints->setColorScheme(scheme);
            },
        .unsetColorScheme = [hints] { hints->unsetColorScheme(); },
    };
}

}

AppearanceModel::AppearanceModel(std::unique_ptr<QSettings> settings, QObject* parent)
    : AppearanceModel(std::move(settings), applicationStyleHooks(), parent)
{
    auto* hints = QGuiApplication::styleHints();
    connect(
        hints,
        &QStyleHints::colorSchemeChanged,
        this,
        [this](const Qt::ColorScheme scheme) {
            updateDark(scheme);
        });
    const auto* accessibility = hints->accessibility();
    connect(
        accessibility,
        &QAccessibilityHints::motionPreferenceChanged,
        this,
        [this](const Qt::MotionPreference motion) {
            setReducedMotion(motion == Qt::MotionPreference::ReducedMotion);
        });
    setReducedMotion(accessibility->motionPreference() == Qt::MotionPreference::ReducedMotion);
}

AppearanceModel::AppearanceModel(
    std::unique_ptr<QSettings> settings,
    StyleHooks styleHooks,
    QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
    , m_styleHooks(std::move(styleHooks))
{
    Q_ASSERT(m_settings != nullptr);
    Q_ASSERT(m_styleHooks.colorScheme);
    Q_ASSERT(m_styleHooks.setColorScheme);
    Q_ASSERT(m_styleHooks.unsetColorScheme);
    m_preference = preferenceFromName(m_settings->value(preferenceKey).toString()).value_or(Preference::System);
    applyPreference();
}

AppearanceModel::Preference AppearanceModel::preference() const noexcept
{
    return m_preference;
}

namespace {
quint64 stableHash(const QString& key)
{
    quint64 hash = 0xCBF29CE484222325ULL;
    for (const auto byte : key.toUtf8()) {
        hash ^= static_cast<quint8>(byte);
        hash *= 0x00000100000001B3ULL;
    }
    return hash;
}
}

int AppearanceModel::stableIndex(const QString& key, const int count)
{
    return count > 0 ? static_cast<int>(stableHash(key) % static_cast<quint64>(count)) : 0;
}

int AppearanceModel::stableBits(const QString& key)
{
    return static_cast<int>(stableHash(key) & 0xFFFFULL);
}

bool AppearanceModel::dark() const noexcept
{
    return m_dark;
}

bool AppearanceModel::reduceMotion() const noexcept
{
    return m_reduceMotion;
}

QString AppearanceModel::settingsError() const
{
    return m_settingsError;
}

bool AppearanceModel::setPreference(const int preference)
{
    if (preference < static_cast<int>(Preference::System)
        || preference > static_cast<int>(Preference::Dark)) {
        setError(tr(
            "Appearance was not saved because the selected preference is invalid."));
        return false;
    }
    const auto candidate = static_cast<Preference>(preference);
    if (candidate == m_preference) {
        if (!m_settingsError.isEmpty() && !persist(candidate)) {
            return false;
        }
        setError({});
        applyPreference();
        return true;
    }
    if (!persist(candidate)) {
        return false;
    }
    m_preference = candidate;
    setError({});
    emit preferenceChanged();
    applyPreference();
    return true;
}

void AppearanceModel::injectSystemColorSchemeForTesting(
    const Qt::ColorScheme scheme)
{
    updateDark(scheme);
}

bool AppearanceModel::persist(const Preference preference)
{
    const auto existed = m_settings->contains(preferenceKey);
    const auto previous = m_settings->value(preferenceKey);
    m_settings->setValue(preferenceKey, preferenceName(preference));
    m_settings->sync();
    if (m_settings->status() == QSettings::NoError) {
        return true;
    }
    if (existed) {
        m_settings->setValue(preferenceKey, previous);
    } else {
        m_settings->remove(preferenceKey);
    }
    m_settings->sync();
    setError(tr(
        "Appearance settings could not be written. The previous preference remains active."));
    return false;
}

void AppearanceModel::applyPreference()
{
    switch (m_preference) {
    case Preference::System:
        m_styleHooks.unsetColorScheme();
        updateDark(m_styleHooks.colorScheme());
        break;
    case Preference::Light:
        m_styleHooks.setColorScheme(Qt::ColorScheme::Light);
        updateDark(Qt::ColorScheme::Light);
        break;
    case Preference::Dark:
        m_styleHooks.setColorScheme(Qt::ColorScheme::Dark);
        updateDark(Qt::ColorScheme::Dark);
        break;
    }
}

void AppearanceModel::updateDark(const Qt::ColorScheme scheme)
{
    const bool dark = m_preference == Preference::Dark
        || (m_preference == Preference::System && scheme == Qt::ColorScheme::Dark);
    if (m_dark == dark) {
        return;
    }
    m_dark = dark;
    emit darkChanged();
}

void AppearanceModel::setReducedMotion(const bool enabled)
{
    if (m_reduceMotion == enabled) {
        return;
    }
    m_reduceMotion = enabled;
    emit reduceMotionChanged();
}

void AppearanceModel::setError(QString error)
{
    if (m_settingsError == error) {
        return;
    }
    m_settingsError = std::move(error);
    emit settingsErrorChanged();
}

}
