#include "presentation/AppearanceModel.hpp"

#include <QDBusConnection>
#include <QDBusVariant>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QTemporaryDir>
#include <QtTest/QTest>

#include <memory>
#include <optional>

namespace {

constexpr auto structuredKey = "appearance/settings.v1";
const auto portalService = QStringLiteral("org.freedesktop.portal.Desktop");
const auto portalPath = QStringLiteral("/org/freedesktop/portal/desktop");

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

class FakePortalSettings final : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.portal.Settings")

public:
    QVariant reducedMotion = QVariant::fromValue<quint32>(0U);
    int readCount = 0;

public slots:
    QDBusVariant ReadOne(const QString& settingsNamespace, const QString& key)
    {
        if (settingsNamespace
                == QStringLiteral("org.freedesktop.appearance")
            && key == QStringLiteral("reduced-motion")) {
            ++readCount;
            return QDBusVariant(reducedMotion);
        }
        return QDBusVariant(QVariant {});
    }

signals:
    void SettingChanged(
        QString settingsNamespace,
        QString key,
        QDBusVariant value);
};

}

class AppearanceModelTest final : public QObject {
    Q_OBJECT

private slots:
    void cleanup();
    void persistsPreferenceAndAppliesStyleOverride();
    void invalidStoredDataFallsBackSilently();
    void followsLiveSystemSchemeAfterUnsettingOverride();
    void portalAbsenceDefaultsToNormalMotion();
    void portalChangesAreStrictlyValidated();
    void portalServiceRestartRefreshesPreference();

private:
    QDBusConnection m_bus = QDBusConnection::sessionBus();
    FakePortalSettings m_portal;

    void registerPortal();
    void unregisterPortal();
};

void AppearanceModelTest::cleanup()
{
    unregisterPortal();
    m_portal.reducedMotion = QVariant::fromValue<quint32>(0U);
    m_portal.readCount = 0;
}

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
            QDBusConnection(QStringLiteral("appearance-no-bus")),
            style.hooks(),
            false);
        QCOMPARE(
            model.preference(),
            kodosi::AppearanceModel::Preference::System);
        QCOMPARE(
            model.effectiveScheme(),
            kodosi::AppearanceModel::EffectiveScheme::DarkScheme);
        QCOMPARE(style.unsetCalls, 1);

        QVERIFY(model.setPreference(
            static_cast<int>(kodosi::AppearanceModel::Preference::Light)));
        QCOMPARE(
            model.preference(),
            kodosi::AppearanceModel::Preference::Light);
        QCOMPARE(
            model.effectiveScheme(),
            kodosi::AppearanceModel::EffectiveScheme::LightScheme);
        QCOMPARE(style.overrideScheme, Qt::ColorScheme::Light);
        QCOMPARE(style.setCalls, 1);
    }

    FakeStyle reloadedStyle;
    reloadedStyle.systemScheme = Qt::ColorScheme::Dark;
    kodosi::AppearanceModel reloaded(
        settingsFor(directory),
        QDBusConnection(QStringLiteral("appearance-no-bus-reload")),
        reloadedStyle.hooks(),
        false);
    QCOMPARE(
        reloaded.preference(),
        kodosi::AppearanceModel::Preference::Light);
    QCOMPARE(
        reloaded.effectiveScheme(),
        kodosi::AppearanceModel::EffectiveScheme::LightScheme);
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
            QString::fromLatin1(structuredKey),
            QStringLiteral("system"));
        storage->sync();
    }
    FakeStyle style;
    kodosi::AppearanceModel invalidType(
        settingsFor(directory),
        QDBusConnection(QStringLiteral("appearance-invalid-type")),
        style.hooks(),
        false);
    QCOMPARE(
        invalidType.preference(),
        kodosi::AppearanceModel::Preference::System);
    QVERIFY(invalidType.settingsError().isEmpty());

    {
        auto storage = settingsFor(directory);
        storage->setValue(
            QString::fromLatin1(structuredKey),
            QJsonDocument(QJsonObject {
                {QStringLiteral("version"), 2},
                {QStringLiteral("preference"), QStringLiteral("dark")},
            }).toJson(QJsonDocument::Compact));
        storage->sync();
    }
    FakeStyle malformedStyle;
    kodosi::AppearanceModel malformed(
        settingsFor(directory),
        QDBusConnection(QStringLiteral("appearance-malformed")),
        malformedStyle.hooks(),
        false);
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
        QDBusConnection(QStringLiteral("appearance-repaired")),
        repairedStyle.hooks(),
        false);
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
        QDBusConnection(QStringLiteral("appearance-system")),
        style.hooks(),
        false);

    QCOMPARE(
        model.effectiveScheme(),
        kodosi::AppearanceModel::EffectiveScheme::LightScheme);
    style.systemScheme = Qt::ColorScheme::Dark;
    model.injectSystemColorSchemeForTesting(Qt::ColorScheme::Dark);
    QCOMPARE(
        model.effectiveScheme(),
        kodosi::AppearanceModel::EffectiveScheme::DarkScheme);

    QVERIFY(model.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::Light)));
    model.injectSystemColorSchemeForTesting(Qt::ColorScheme::Dark);
    QCOMPARE(
        model.effectiveScheme(),
        kodosi::AppearanceModel::EffectiveScheme::LightScheme);

    QVERIFY(model.setPreference(
        static_cast<int>(kodosi::AppearanceModel::Preference::System)));
    QCOMPARE(style.unsetCalls, 2);
    QCOMPARE(
        model.effectiveScheme(),
        kodosi::AppearanceModel::EffectiveScheme::DarkScheme);
}

void AppearanceModelTest::portalAbsenceDefaultsToNormalMotion()
{
    QVERIFY(m_bus.isConnected());
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-absent-XXXXXX")));
    QVERIFY(directory.isValid());
    FakeStyle style;
    kodosi::AppearanceModel model(
        settingsFor(directory),
        m_bus,
        style.hooks(),
        true);
    QTest::qWait(30);
    QVERIFY(!model.reduceMotion());

    m_portal.reducedMotion = QVariant::fromValue<quint32>(1U);
    registerPortal();
    QTRY_VERIFY(model.reduceMotion());
}

void AppearanceModelTest::portalChangesAreStrictlyValidated()
{
    registerPortal();
    m_portal.reducedMotion = QVariant::fromValue<quint32>(0U);
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-portal-XXXXXX")));
    QVERIFY(directory.isValid());
    FakeStyle style;
    kodosi::AppearanceModel model(
        settingsFor(directory),
        m_bus,
        style.hooks(),
        true);
    QTRY_VERIFY(m_portal.readCount >= 1);
    QVERIFY(!model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.example.unrelated"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(QVariant::fromValue<quint32>(1U)));
    QTest::qWait(20);
    QVERIFY(!model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("contrast"),
        QDBusVariant(QVariant::fromValue<quint32>(1U)));
    QTest::qWait(20);
    QVERIFY(!model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(true));
    QTest::qWait(20);
    QVERIFY(!model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(1));
    QTest::qWait(20);
    QVERIFY(!model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(QVariant::fromValue<quint32>(1U)));
    QTRY_VERIFY(model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(QVariant::fromValue<quint32>(2U)));
    QTRY_VERIFY(!model.reduceMotion());
}

void AppearanceModelTest::portalServiceRestartRefreshesPreference()
{
    registerPortal();
    m_portal.reducedMotion = QVariant::fromValue<quint32>(1U);
    QTemporaryDir directory(
        QDir::current().filePath(QStringLiteral("appearance-restart-XXXXXX")));
    QVERIFY(directory.isValid());
    FakeStyle style;
    kodosi::AppearanceModel model(
        settingsFor(directory),
        m_bus,
        style.hooks(),
        true);
    QTRY_VERIFY(model.reduceMotion());
    const auto initialReads = m_portal.readCount;

    unregisterPortal();
    QTRY_VERIFY(!model.reduceMotion());

    m_portal.reducedMotion = QVariant::fromValue<quint32>(1U);
    registerPortal();
    QTRY_VERIFY(m_portal.readCount > initialReads);
    QTRY_VERIFY(model.reduceMotion());

    emit m_portal.SettingChanged(
        QStringLiteral("org.freedesktop.appearance"),
        QStringLiteral("reduced-motion"),
        QDBusVariant(QVariant::fromValue<quint32>(0U)));
    QTRY_VERIFY(!model.reduceMotion());
}

void AppearanceModelTest::registerPortal()
{
    QVERIFY(m_bus.registerService(portalService));
    QVERIFY(m_bus.registerObject(
        portalPath,
        &m_portal,
        QDBusConnection::ExportAllSlots
            | QDBusConnection::ExportAllSignals));
}

void AppearanceModelTest::unregisterPortal()
{
    m_bus.unregisterObject(portalPath);
    m_bus.unregisterService(portalService);
}

QTEST_GUILESS_MAIN(AppearanceModelTest)

#include "tst_appearance_model.moc"
