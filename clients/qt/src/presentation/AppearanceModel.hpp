#pragma once

#include <QObject>
#include <QSettings>
#include <QString>

#include <functional>
#include <memory>

namespace kodosi {

class AppearanceModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(Preference preference READ preference NOTIFY preferenceChanged)
    Q_PROPERTY(bool dark READ dark NOTIFY darkChanged)
    Q_PROPERTY(bool reduceMotion READ reduceMotion NOTIFY reduceMotionChanged)
    Q_PROPERTY(bool systemReducesMotion READ systemReducesMotion NOTIFY reduceMotionChanged)
    Q_PROPERTY(QString settingsError READ settingsError NOTIFY settingsErrorChanged)

public:
    enum class Preference {
        System = 0,
        Light = 1,
        Dark = 2,
    };
    Q_ENUM(Preference)

    struct StyleHooks {
        std::function<Qt::ColorScheme()> colorScheme;
        std::function<void(Qt::ColorScheme)> setColorScheme;
        std::function<void()> unsetColorScheme;
    };

    explicit AppearanceModel(std::unique_ptr<QSettings> settings, QObject* parent = nullptr);
    AppearanceModel(
        std::unique_ptr<QSettings> settings,
        StyleHooks styleHooks,
        QObject* parent = nullptr);

    [[nodiscard]] Preference preference() const noexcept;
    [[nodiscard]] bool dark() const noexcept;
    [[nodiscard]] bool reduceMotion() const noexcept;
    [[nodiscard]] bool systemReducesMotion() const noexcept;
    [[nodiscard]] QString settingsError() const;

    Q_INVOKABLE [[nodiscard]] bool setPreference(int preference);
    Q_INVOKABLE [[nodiscard]] bool setReducesMotion(bool enabled);
    Q_INVOKABLE [[nodiscard]] static int stableIndex(const QString& key, int count);
    Q_INVOKABLE [[nodiscard]] static int stableBits(const QString& key);

    void injectSystemColorSchemeForTesting(Qt::ColorScheme scheme);
    void injectSystemReducedMotionForTesting(bool enabled);

signals:
    void preferenceChanged();
    void darkChanged();
    void reduceMotionChanged();
    void settingsErrorChanged();

private:
    std::unique_ptr<QSettings> m_settings;
    StyleHooks m_styleHooks;
    Preference m_preference = Preference::System;
    bool m_dark = false;
    bool m_systemReducesMotion = false;
    bool m_reducesMotion = false;
    QString m_settingsError;

    [[nodiscard]] bool store(const QString& key, const QVariant& value);
    [[nodiscard]] bool persist(Preference preference);
    void applyPreference();
    void updateDark(Qt::ColorScheme scheme);
    void updateMotion(bool system, bool chosen);
    void setError(QString error);
};

}
