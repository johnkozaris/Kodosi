#pragma once

#include <QObject>
#include <QSettings>
#include <QString>
#include <QVariantMap>

#include <functional>
#include <memory>

namespace kodosi {

class AppearanceModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(Preference preference READ preference NOTIFY preferenceChanged FINAL)
    Q_PROPERTY(bool dark READ dark NOTIFY darkChanged FINAL)
    Q_PROPERTY(bool keepsTerminalsDark READ keepsTerminalsDark NOTIFY terminalLookChanged FINAL)
    Q_PROPERTY(QString terminalLook READ terminalLook NOTIFY terminalLookChanged FINAL)
    Q_PROPERTY(bool reduceMotion READ reduceMotion NOTIFY reduceMotionChanged FINAL)
    Q_PROPERTY(bool systemReducesMotion READ systemReducesMotion NOTIFY reduceMotionChanged FINAL)
    Q_PROPERTY(bool summaryFirst READ summaryFirst NOTIFY summaryFirstChanged FINAL)
    Q_PROPERTY(TextSize textSize READ textSize NOTIFY textSizeChanged FINAL)
    Q_PROPERTY(qreal textScale READ textScale NOTIFY textSizeChanged FINAL)
    Q_PROPERTY(QString settingsError READ settingsError NOTIFY settingsErrorChanged FINAL)

public:
    enum class Preference {
        System = 0,
        Light = 1,
        Dark = 2,
    };
    Q_ENUM(Preference)

    enum class TextSize {
        Small = 0,
        Normal = 1,
        Large = 2,
        Larger = 3,
    };
    Q_ENUM(TextSize)

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
    [[nodiscard]] bool keepsTerminalsDark() const noexcept;
    [[nodiscard]] QString terminalLook() const;
    [[nodiscard]] bool reduceMotion() const noexcept;
    [[nodiscard]] bool systemReducesMotion() const noexcept;
    [[nodiscard]] bool summaryFirst() const noexcept;
    [[nodiscard]] TextSize textSize() const noexcept;
    /// How much larger than normal the words and the controls around them are: about one point a
    /// step at the size of the conversation.
    [[nodiscard]] qreal textScale() const noexcept;
    [[nodiscard]] QString settingsError() const;

    Q_INVOKABLE [[nodiscard]] bool setPreference(int preference);
    Q_INVOKABLE [[nodiscard]] bool setKeepsTerminalsDark(bool enabled);
    Q_INVOKABLE [[nodiscard]] bool setReducesMotion(bool enabled);
    Q_INVOKABLE [[nodiscard]] bool setSummaryFirst(bool enabled);
    Q_INVOKABLE [[nodiscard]] bool setTextSize(int size);
    Q_INVOKABLE [[nodiscard]] static int stableIndex(const QString& key, int count);
    Q_INVOKABLE [[nodiscard]] static int stableBits(const QString& key);
    /// A list puts an agent's summary before the terminal's name only while the line below can
    /// still name the terminal, rather than say its state.
    Q_INVOKABLE [[nodiscard]] static QString leadingSummary(const QVariantMap& session, bool summaryFirst);

    void followSystemColorScheme(Qt::ColorScheme scheme);
    void followSystemMotion(bool reduced);

signals:
    void preferenceChanged();
    void darkChanged();
    void terminalLookChanged();
    void reduceMotionChanged();
    void summaryFirstChanged();
    void textSizeChanged();
    void settingsErrorChanged();

private:
    std::unique_ptr<QSettings> m_settings;
    StyleHooks m_styleHooks;
    Preference m_preference = Preference::System;
    bool m_dark = false;
    bool m_keepsTerminalsDark = false;
    bool m_systemReducesMotion = false;
    bool m_reducesMotion = false;
    bool m_summaryFirst = false;
    TextSize m_textSize = TextSize::Normal;
    QString m_settingsError;

    [[nodiscard]] bool store(const QString& key, const QVariant& value);
    [[nodiscard]] bool persist(Preference preference);
    void applyPreference();
    void updateDark(Qt::ColorScheme scheme);
    void updateMotion(bool system, bool chosen);
    void setError(QString error);
};

}
