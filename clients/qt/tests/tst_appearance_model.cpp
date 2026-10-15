#include "presentation/AppearanceModel.hpp"

#include <QDir>
#include <QSettings>
#include <QTemporaryDir>
#include <QtTest/QTest>

#include <memory>
#include <optional>

namespace {

constexpr auto preferenceKey = "appearance/preference";

std::unique_ptr<QSettings> settingsFor(const QTemporaryDir& directory)
{
    return std::make_unique<QSettings>(
        directory.filePath(QStringLiteral("settings.ini")),
        QSettings::IniFormat);
}

class FakeStyle final {
public:
    Qt::ColorScheme systemScheme = Qt::ColorScheme::Light;
    std::optional<Qt::ColorScheme> overrideScheme;
    int setCalls = 0;
    int unsetCalls = 0;

    kodosi::AppearanceModel::StyleHooks hooks()
    {
        return {
            .colorScheme = [this] {
                return overrideScheme.value_or(systemScheme);
            },
            .setColorScheme =
                [this](const Qt::ColorScheme scheme) {
                    ++setCalls;
                    overrideScheme = scheme;
                },
            .unsetColorScheme = [this] {
                ++unsetCalls;
                overrideScheme.reset();
            },
        };
    }
};

}

class AppearanceModelTest final : public QObject {
    Q_OBJECT

private slots:
    void persistsPreferenceAndAppliesStyleOverride();
    void invalidStoredDataFallsBackSilently();
    void followsLiveSystemSchemeAfterUnsettingOverride();
};

void AppearanceModelTest::persistsPreferenceAndAppliesStyleOverride()
{
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-persist-XXXXXX")));
    QVERIFY(directory.isValid());
    FakeStyle style;
    style.systemScheme = Qt::ColorScheme::Dark;

    {
        kodosi::AppearanceModel model(
            settingsFor(directory),
            style.hooks());
        QCOMPARE(
            model.preference(),
            kodosi::AppearanceModel::Preference::System);
        QVERIFY(model.dark());
        QCOMPARE(style.unsetCalls, 1);

        QVERIFY(model.setPreference(
            static_cast<int>(kodosi::AppearanceModel::Preference::Light)));
        QCOMPARE(
            model.preference(),
            kodosi::AppearanceModel::Preference::Light);
        QVERIFY(!model.dark());
        QCOMPARE(style.overrideScheme, Qt::ColorScheme::Light);
        QCOMPARE(style.setCalls, 1);
    }

    FakeStyle reloadedStyle;
    reloadedStyle.systemScheme = Qt::ColorScheme::Dark;
    kodosi::AppearanceModel reloaded(
        settingsFor(directory),
        reloadedStyle.hooks());
    QCOMPARE(
        reloaded.preference(),
        kodosi::AppearanceModel::Preference::Light);
    QVERIFY(!reloaded.dark());
    QCOMPARE(reloadedStyle.overrideScheme, Qt::ColorScheme::Light);
    QVERIFY(reloaded.settingsError().isEmpty());

    QVERIFY(reloaded.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::System)));
    QVERIFY(!reloadedStyle.overrideScheme.has_value());
    QCOMPARE(reloadedStyle.unsetCalls, 1);
}

void AppearanceModelTest::invalidStoredDataFallsBackSilently()
{
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-invalid-XXXXXX")));
    QVERIFY(directory.isValid());
    {
        auto storage = settingsFor(directory);
        storage->setValue(
            QString::fromLatin1(preferenceKey),
            QStringLiteral("purple"));
        storage->sync();
    }
    FakeStyle malformedStyle;
    kodosi::AppearanceModel malformed(
        settingsFor(directory),
        malformedStyle.hooks());
    QCOMPARE(
        malformed.preference(),
        kodosi::AppearanceModel::Preference::System);
    QVERIFY(malformed.settingsError().isEmpty());

    QVERIFY(!malformed.setPreference(99));
    QVERIFY(malformed.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::System)));
    QVERIFY(malformed.settingsError().isEmpty());

    FakeStyle repairedStyle;
    kodosi::AppearanceModel repaired(
        settingsFor(directory),
        repairedStyle.hooks());
    QCOMPARE(
        repaired.preference(),
        kodosi::AppearanceModel::Preference::System);
    QVERIFY(repaired.settingsError().isEmpty());
}

void AppearanceModelTest::followsLiveSystemSchemeAfterUnsettingOverride()
{
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-system-XXXXXX")));
    QVERIFY(directory.isValid());
    FakeStyle style;
    style.systemScheme = Qt::ColorScheme::Light;
    kodosi::AppearanceModel model(
        settingsFor(directory),
        style.hooks());

    QVERIFY(!model.dark());
    style.systemScheme = Qt::ColorScheme::Dark;
    model.injectSystemColorSchemeForTesting(Qt::ColorScheme::Dark);
    QVERIFY(model.dark());

    QVERIFY(model.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::Light)));
    model.injectSystemColorSchemeForTesting(Qt::ColorScheme::Dark);
    QVERIFY(!model.dark());

    QVERIFY(model.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::System)));
    QCOMPARE(style.unsetCalls, 2);
    QVERIFY(model.dark());
}

QTEST_GUILESS_MAIN(AppearanceModelTest)

#include "tst_appearance_model.moc"
