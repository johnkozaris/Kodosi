#include "presentation/AppearanceModel.hpp"

#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusServiceWatcher>
#include <QDBusVariant>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaType>
#include <QStyleHints>

#include <optional>
#include <utility>

namespace kodosi {
namespace {

constexpr auto settingsKey = "appearance/settings.v1";
constexpr auto currentVersion = 1;
const auto portalService = QStringLiteral("org.freedesktop.portal.Desktop");
const auto portalPath = QStringLiteral("/org/freedesktop/portal/desktop");
const auto portalInterface = QStringLiteral("org.freedesktop.portal.Settings");
const auto appearanceNamespace = QStringLiteral("org.freedesktop.appearance");
const auto reducedMotionKey = QStringLiteral("reduced-motion");

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

std::optional<AppearanceModel::Preference> preferenceFromName(
    const QString& name)
{
    if (name == QStringLiteral("system")) {
        return AppearanceModel::Preference::System;
    }
    if (name == QStringLiteral("light")) {
        return AppearanceModel::Preference::Light;
    }
    if (name == QStringLiteral("dark")) {
        return AppearanceModel::Preference::Dark;
    }
    return std::nullopt;
}

QByteArray encode(const AppearanceModel::Preference preference)
{
    return QJsonDocument(QJsonObject {
        {QStringLiteral("version"), currentVersion},
        {QStringLiteral("preference"), preferenceName(preference)},
    }).toJson(QJsonDocument::Compact);
}

std::optional<AppearanceModel::Preference> decode(const QByteArray& json)
{
    QJsonParseError error;
    const auto document = QJsonDocument::fromJson(json, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject()) {
        return std::nullopt;
    }
    const auto root = document.object();
    const auto version = root.value(QStringLiteral("version"));
    const auto preference = root.value(QStringLiteral("preference"));
    if (root.size() != 2 || !version.isDouble()
        || version.toDouble() != currentVersion || !preference.isString()) {
        return std::nullopt;
    }
    return preferenceFromName(preference.toString());
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

AppearanceModel::AppearanceModel(QObject* parent)
    : AppearanceModel(std::make_unique<QSettings>(), parent)
{
}

AppearanceModel::AppearanceModel(std::unique_ptr<QSettings> settings, QObject* parent)
    : AppearanceModel(
          std::move(settings),
          QDBusConnection::sessionBus(),
          applicationStyleHooks(),
          true,
          parent)
{
    auto* hints = QGuiApplication::styleHints();
    connect(
        hints,
        &QStyleHints::colorSchemeChanged,
        this,
        [this](const Qt::ColorScheme scheme) {
            updateEffectiveScheme(scheme);
        });
}

AppearanceModel::AppearanceModel(
    std::unique_ptr<QSettings> settings,
    QDBusConnection bus,
    StyleHooks styleHooks,
    const bool monitorPortal,
    QObject* parent)
    : QObject(parent)
    , m_settings(std::move(settings))
    , m_bus(std::move(bus))
    , m_styleHooks(std::move(styleHooks))
    , m_monitorPortal(monitorPortal)
{
    Q_ASSERT(m_settings != nullptr);
    Q_ASSERT(m_styleHooks.colorScheme);
    Q_ASSERT(m_styleHooks.setColorScheme);
    Q_ASSERT(m_styleHooks.unsetColorScheme);
    load();
    applyPreference();
    if (m_monitorPortal) {
        startPortalMonitoring();
    }
}

AppearanceModel::~AppearanceModel() = default;

AppearanceModel::Preference AppearanceModel::preference() const noexcept
{
    return m_preference;
}

AppearanceModel::EffectiveScheme AppearanceModel::effectiveScheme() const noexcept
{
    return m_effectiveScheme;
}

bool AppearanceModel::dark() const noexcept
{
    return m_effectiveScheme == EffectiveScheme::DarkScheme;
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

void AppearanceModel::clearError()
{
    setError({});
}

void AppearanceModel::injectSystemColorSchemeForTesting(
    const Qt::ColorScheme scheme)
{
    updateEffectiveScheme(scheme);
}

void AppearanceModel::injectReducedMotionForTesting(const bool enabled)
{
    m_reducedMotionTestOverride = enabled;
    setReducedMotion(enabled);
}

void AppearanceModel::onPortalSettingChanged(
    const QString& settingsNamespace,
    const QString& key,
    const QDBusVariant& value)
{
    if (settingsNamespace != appearanceNamespace || key != reducedMotionKey) {
        return;
    }
    handleReducedMotionValue(value.variant());
}

void AppearanceModel::load()
{
    m_preference = Preference::System;
    const auto stored =
        m_settings->value(QString::fromLatin1(settingsKey));
    if (!stored.isValid()) {
        return;
    }
    if (stored.metaType() != QMetaType::fromType<QByteArray>()) {
        return;
    }
    const auto decoded = decode(stored.toByteArray());
    if (!decoded) {
        return;
    }
    m_preference = *decoded;
}

bool AppearanceModel::persist(const Preference preference)
{
    const auto key = QString::fromLatin1(settingsKey);
    const auto existed = m_settings->contains(key);
    const auto previous = m_settings->value(key);
    m_settings->setValue(key, encode(preference));
    m_settings->sync();
    if (m_settings->status() == QSettings::NoError) {
        return true;
    }
    if (existed) {
        m_settings->setValue(key, previous);
    } else {
        m_settings->remove(key);
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
        updateEffectiveScheme(m_styleHooks.colorScheme());
        break;
    case Preference::Light:
        m_styleHooks.setColorScheme(Qt::ColorScheme::Light);
        updateEffectiveScheme(Qt::ColorScheme::Light);
        break;
    case Preference::Dark:
        m_styleHooks.setColorScheme(Qt::ColorScheme::Dark);
        updateEffectiveScheme(Qt::ColorScheme::Dark);
        break;
    }
}

void AppearanceModel::updateEffectiveScheme(const Qt::ColorScheme scheme)
{
    const auto effective = m_preference == Preference::Dark
        || (m_preference == Preference::System
            && scheme == Qt::ColorScheme::Dark)
        ? EffectiveScheme::DarkScheme
        : EffectiveScheme::LightScheme;
    if (m_effectiveScheme == effective) {
        return;
    }
    m_effectiveScheme = effective;
    emit effectiveSchemeChanged();
}

void AppearanceModel::setReducedMotion(const bool enabled)
{
    const auto effective =
        m_reducedMotionTestOverride.value_or(enabled);
    if (m_reduceMotion == effective) {
        return;
    }
    m_reduceMotion = effective;
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

void AppearanceModel::startPortalMonitoring()
{
    if (!m_bus.isConnected()) {
        setReducedMotion(false);
        return;
    }
    m_serviceWatcher = std::make_unique<QDBusServiceWatcher>(
        portalService,
        m_bus,
        QDBusServiceWatcher::WatchForOwnerChange,
        this);
    connect(
        m_serviceWatcher.get(),
        &QDBusServiceWatcher::serviceOwnerChanged,
        this,
        [this](
            const QString&,
            const QString& oldOwner,
            const QString& newOwner) {
            if (oldOwner == newOwner) {
                return;
            }
            ++m_portalGeneration;
            if (newOwner.isEmpty()) {
                setReducedMotion(false);
                return;
            }
            requestReducedMotion();
        });
    (void)m_bus.connect(
        portalService,
        portalPath,
        portalInterface,
        QStringLiteral("SettingChanged"),
        this,
        SLOT(onPortalSettingChanged(QString,QString,QDBusVariant)));
    requestReducedMotion();
}

void AppearanceModel::requestReducedMotion()
{
    const auto generation = ++m_portalGeneration;
    auto request = QDBusMessage::createMethodCall(
        portalService,
        portalPath,
        portalInterface,
        QStringLiteral("ReadOne"));
    request.setArguments({appearanceNamespace, reducedMotionKey});
    auto* watcher =
        new QDBusPendingCallWatcher(m_bus.asyncCall(request), this);
    connect(
        watcher,
        &QDBusPendingCallWatcher::finished,
        this,
        [this, generation](QDBusPendingCallWatcher* call) {
            const QDBusPendingReply<QDBusVariant> reply = *call;
            call->deleteLater();
            if (generation != m_portalGeneration) {
                return;
            }
            if (reply.isError()) {
                setReducedMotion(false);
                return;
            }
            handleReducedMotionValue(reply.value().variant());
        });
}

void AppearanceModel::handleReducedMotionValue(const QVariant& value)
{
    if (value.metaType() != QMetaType::fromType<quint32>()) {
        setReducedMotion(false);
        return;
    }
    setReducedMotion(value.value<quint32>() == 1U);
}

}
