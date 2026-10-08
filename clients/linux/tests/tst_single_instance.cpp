#include "app/SingleInstanceGuard.hpp"
#include "app/RuntimeStorageBootstrap.hpp"

#include <QCryptographicHash>
#include <QDataStream>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QtTest/QTest>

#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <future>
#include <functional>
#include <memory>
#include <thread>
#include <vector>

namespace {

class EnvironmentGuard final {
public:
    explicit EnvironmentGuard(const char* name)
        : m_name(name)
        , m_wasSet(qEnvironmentVariableIsSet(name))
        , m_value(qgetenv(name))
    {
    }

    ~EnvironmentGuard()
    {
        if (m_wasSet) {
            (void)qputenv(m_name.constData(), m_value);
        } else {
            qunsetenv(m_name.constData());
        }
    }

private:
    QByteArray m_name;
    bool m_wasSet;
    QByteArray m_value;
};

kodosi::ApplicationActivation routeActivation(
    const QString& requestId,
    const QString& activationToken = {})
{
    return {
        .kind = kodosi::ApplicationActivation::Kind::Route,
        .requestId = requestId,
        .destination = kodosi::DeepLinkDestination {
            .sessionId = QStringLiteral("session-1"),
        },
        .activationToken = activationToken,
    };
}

QString namespaceForRoot(const QString& root)
{
    return QString::fromLatin1(
        QCryptographicHash::hash(
            root.toUtf8(),
            QCryptographicHash::Sha256)
            .toHex()
            .left(16));
}

QByteArray rawFrame(const QByteArray& payload)
{
    QByteArray framed;
    QDataStream stream(&framed, QIODevice::WriteOnly);
    stream.setByteOrder(QDataStream::BigEndian);
    stream << static_cast<quint32>(payload.size());
    framed.append(payload);
    return framed;
}

bool sendRaw(const QString& endpoint, const QByteArray& bytes)
{
    QLocalSocket socket;
    socket.connectToServer(endpoint);
    if (!socket.waitForConnected(2000)
        || socket.write(bytes) != bytes.size()
        || !socket.waitForBytesWritten(2000)) {
        return false;
    }
    return socket.waitForReadyRead(2000)
        && !socket.readAll().isEmpty();
}

}

class SingleInstanceTest final : public QObject {
    Q_OBJECT

private slots:
    void endpointNamespaceMatchesEffectiveRustDataRoot();
    void storageBootstrapIsolatesPreferencesWithoutChangingProviderHome();
    void storageBootstrapRejectsOverlapsBeforeCreatingFiles();
    void endpointNamespaceRejectsUnsafeIsolatedRoots();
    void endpointNamespaceRejectsCoreSymlinkAndProductionAliases();
    void delaysEndpointUntilOwnerReadinessAndDeliversExactlyOnce();
    void contenderTakesOwnershipOnlyAfterUnreadyOwnerFails();
    void contenderUsesOneAbsoluteReadinessTimeout();
    void forwardsAcknowledgesAndDeduplicates();
    void forwardsBoundedActivationTokensForEveryKind();
    void rejectsUnsafeActivationTokensAndScopesOwnerHook();
    void rejectsMalformedAndOversizedFrames();
    void boundsIncompleteAcceptedClients();
    void shortEndpointNamesFitPortableUnixSocketPaths();
    void removesOnlyUnlockedStaleEndpoint();
};

void SingleInstanceTest::storageBootstrapIsolatesPreferencesWithoutChangingProviderHome()
{
    EnvironmentGuard data("KODOSI_DATA_ROOT");
    EnvironmentGuard production("KODOSI_PRODUCTION_DATA_ROOT");
    EnvironmentGuard config("XDG_CONFIG_HOME");
    QTemporaryDir directory;
    QVERIFY(directory.isValid());
    const auto root = directory.filePath(QStringLiteral("isolated"));
    const auto protectedRoot = directory.filePath(QStringLiteral("production"));
    const auto originalHome = qgetenv("HOME");
    const auto originalConfig = directory.filePath(QStringLiteral("config"));
    qputenv("XDG_CONFIG_HOME", originalConfig.toUtf8());
    qputenv("KODOSI_DATA_ROOT", root.toUtf8());
    qputenv("KODOSI_PRODUCTION_DATA_ROOT", protectedRoot.toUtf8());
    const auto storage = kodosi::RuntimeStorageBootstrap::resolve();
    QVERIFY2(storage, storage ? "" : qPrintable(storage.error()));
    QVERIFY(!QFileInfo::exists(root));
    QVERIFY(storage->prepare());
    QCOMPARE(storage->runtimeRoot, root + QStringLiteral("/core"));
    QCOMPARE(storage->logDirectory, root + QStringLiteral("/qt/logs"));
    auto settings = storage->settings();
    settings->setValue(QStringLiteral("terminal/fontSize"), 19);
    settings->sync();
    QCOMPARE(settings->status(), QSettings::NoError);
    QCOMPARE(storage->settings()->value(QStringLiteral("terminal/fontSize")).toInt(), 19);
    QVERIFY(!settings->fallbacksEnabled());
    QCOMPARE(qgetenv("HOME"), originalHome);
    QCOMPARE(qgetenv("XDG_CONFIG_HOME"), originalConfig.toUtf8());
    QVERIFY(!QFileInfo::exists(originalConfig));
    QVERIFY(!QFileInfo::exists(protectedRoot));
    struct stat status {};
    QVERIFY(::stat(QFile::encodeName(storage->preferencesPath).constData(), &status) == 0);
    QCOMPARE(status.st_mode & 0777, static_cast<mode_t>(0600));
}

void SingleInstanceTest::storageBootstrapRejectsOverlapsBeforeCreatingFiles()
{
    EnvironmentGuard data("KODOSI_DATA_ROOT");
    EnvironmentGuard production("KODOSI_PRODUCTION_DATA_ROOT");
    QTemporaryDir directory;
    QVERIFY(directory.isValid());
    const auto isolated = directory.filePath(QStringLiteral("isolated"));
    qputenv("KODOSI_DATA_ROOT", isolated.toUtf8());
    qputenv("KODOSI_PRODUCTION_DATA_ROOT", (isolated + QStringLiteral("/production")).toUtf8());
    QVERIFY(!kodosi::RuntimeStorageBootstrap::resolve());
    QVERIFY(!QFileInfo::exists(isolated));
    {
        EnvironmentGuard home("HOME");
        qputenv("HOME", (isolated + QStringLiteral("/home")).toUtf8());
        qputenv("KODOSI_PRODUCTION_DATA_ROOT", directory.filePath(QStringLiteral("elsewhere")).toUtf8());
        QVERIFY(!kodosi::RuntimeStorageBootstrap::resolve());
        QVERIFY(!QFileInfo::exists(isolated));
    }
    const auto target = directory.filePath(QStringLiteral("target"));
    const auto alias = directory.filePath(QStringLiteral("alias"));
    QVERIFY(QDir().mkpath(target));
    QVERIFY(::symlink(QFile::encodeName(target).constData(), QFile::encodeName(alias).constData()) == 0);
    qputenv("KODOSI_DATA_ROOT", target.toUtf8());
    qputenv("KODOSI_PRODUCTION_DATA_ROOT", (alias + QStringLiteral("/missing/production")).toUtf8());
    QVERIFY(!kodosi::RuntimeStorageBootstrap::resolve());
    QVERIFY(!QFileInfo::exists(target + QStringLiteral("/qt")));
    qputenv("KODOSI_DATA_ROOT", isolated.toUtf8());
    qputenv("KODOSI_PRODUCTION_DATA_ROOT", target.toUtf8());
    auto safe = kodosi::RuntimeStorageBootstrap::resolve();
    QVERIFY(safe);
    QVERIFY(QDir().mkpath(isolated));
    const auto qtPath = isolated + QStringLiteral("/qt");
    QVERIFY(::symlink(QFile::encodeName(target).constData(), QFile::encodeName(qtPath).constData()) == 0);
    QVERIFY(!safe->prepare());
    QVERIFY(!QFileInfo::exists(target + QStringLiteral("/preferences.ini")));
    QVERIFY(!QFileInfo::exists(target + QStringLiteral("/logs")));
    QVERIFY(QFile::remove(qtPath));
    QVERIFY(QDir().mkpath(qtPath));
    const auto protectedFile = target + QStringLiteral("/provider.json");
    QFile original(protectedFile);
    QVERIFY(original.open(QIODevice::WriteOnly));
    QCOMPARE(original.write("untouched"), 9);
    original.close();
    QVERIFY(::link(QFile::encodeName(protectedFile).constData(),
        QFile::encodeName(qtPath + QStringLiteral("/preferences.ini")).constData()) == 0);
    QVERIFY(!kodosi::RuntimeStorageBootstrap::resolve());
    QVERIFY(original.open(QIODevice::ReadOnly));
    QCOMPARE(original.readAll(), QByteArrayLiteral("untouched"));
    qunsetenv("KODOSI_DATA_ROOT");
    qputenv("KODOSI_PRODUCTION_DATA_ROOT", target.toUtf8());
    const auto productionStorage = kodosi::RuntimeStorageBootstrap::resolve();
    QVERIFY(productionStorage);
    QCOMPARE(productionStorage->runtimeRoot, target);
    QCOMPARE(kodosi::SingleInstanceGuard::endpointNamespace().value, namespaceForRoot(target));
}

void SingleInstanceTest::endpointNamespaceMatchesEffectiveRustDataRoot()
{
    EnvironmentGuard dataRoot("KODOSI_DATA_ROOT");
    EnvironmentGuard productionRoot("KODOSI_PRODUCTION_DATA_ROOT");
    EnvironmentGuard configHome("XDG_CONFIG_HOME");
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-namespace-XXXXXX")));
    QVERIFY(directory.isValid());

    const auto production = QDir(directory.path()).filePath(
        QStringLiteral("production"));
    const auto isolated = QDir(directory.path()).filePath(
        QStringLiteral("isolated"));
    QVERIFY(QDir().mkpath(production));
    QVERIFY(qputenv("KODOSI_DATA_ROOT", isolated.toUtf8()));
    QVERIFY(qputenv(
        "KODOSI_PRODUCTION_DATA_ROOT",
        production.toUtf8()));
    const auto isolatedResult =
        kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY2(isolatedResult.valid(), qPrintable(isolatedResult.error));
    QCOMPARE(
        isolatedResult.value,
        namespaceForRoot(
            QDir::cleanPath(isolated)
            + QStringLiteral("/core")));

    QVERIFY(qputenv(
        "KODOSI_DATA_ROOT",
        (isolated + QStringLiteral("/./")).toUtf8()));
    const auto equivalentResult =
        kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY2(equivalentResult.valid(), qPrintable(equivalentResult.error));
    QCOMPARE(equivalentResult.value, isolatedResult.value);

    qunsetenv("KODOSI_DATA_ROOT");
    qunsetenv("KODOSI_PRODUCTION_DATA_ROOT");
    const auto profileOne = QDir(directory.path()).filePath(
        QStringLiteral("profile-one"));
    const auto profileTwo = QDir(directory.path()).filePath(
        QStringLiteral("profile-two"));
    QVERIFY(qputenv("XDG_CONFIG_HOME", profileOne.toUtf8()));
    const auto firstProfile =
        kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY2(firstProfile.valid(), qPrintable(firstProfile.error));
    QCOMPARE(
        firstProfile.value,
        namespaceForRoot(
            QDir::cleanPath(profileOne)
            + QStringLiteral("/kodosi")));

    QVERIFY(qputenv("XDG_CONFIG_HOME", profileTwo.toUtf8()));
    const auto secondProfile =
        kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY2(secondProfile.valid(), qPrintable(secondProfile.error));
    QVERIFY(firstProfile.value != secondProfile.value);
}

void SingleInstanceTest::endpointNamespaceRejectsUnsafeIsolatedRoots()
{
    EnvironmentGuard dataRoot("KODOSI_DATA_ROOT");
    EnvironmentGuard productionRoot("KODOSI_PRODUCTION_DATA_ROOT");
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-roots-XXXXXX")));
    QVERIFY(directory.isValid());
    const auto production = QDir(directory.path()).filePath(
        QStringLiteral("production"));
    QVERIFY(QDir().mkpath(production));

    QVERIFY(qputenv(
        "KODOSI_DATA_ROOT",
        QDir(directory.path()).filePath(
            QStringLiteral("isolated")).toUtf8()));
    qunsetenv("KODOSI_PRODUCTION_DATA_ROOT");
    auto result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(
        QStringLiteral("KODOSI_PRODUCTION_DATA_ROOT")));

    QVERIFY(qputenv(
        "KODOSI_PRODUCTION_DATA_ROOT",
        production.toUtf8()));
    QVERIFY(qputenv("KODOSI_DATA_ROOT", QByteArrayLiteral("/")));
    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("filesystem root")));

    QVERIFY(qputenv("KODOSI_DATA_ROOT", QByteArrayLiteral("relative")));
    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("absolute")));

    QVERIFY(qputenv(
        "KODOSI_DATA_ROOT",
        (directory.path() + QStringLiteral("/../unsafe")).toUtf8()));
    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("parent-directory")));

    QVERIFY(qputenv("KODOSI_DATA_ROOT", production.toUtf8()));
    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("production")));

    QVERIFY(qputenv(
        "KODOSI_DATA_ROOT",
        QDir(production).filePath(
            QStringLiteral("descendant")).toUtf8()));
    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("production")));
}

void SingleInstanceTest::
    endpointNamespaceRejectsCoreSymlinkAndProductionAliases()
{
    EnvironmentGuard dataRoot("KODOSI_DATA_ROOT");
    EnvironmentGuard productionRoot("KODOSI_PRODUCTION_DATA_ROOT");
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-core-alias-XXXXXX")));
    QVERIFY(directory.isValid());

    const auto production = QDir(directory.path()).filePath(
        QStringLiteral("production"));
    const auto isolated = QDir(directory.path()).filePath(
        QStringLiteral("isolated"));
    const auto core = QDir(isolated).filePath(QStringLiteral("core"));
    QVERIFY(QDir().mkpath(production));
    QVERIFY(QDir().mkpath(isolated));
    QVERIFY(::symlink(
        QFile::encodeName(production).constData(),
        QFile::encodeName(core).constData()) == 0);
    QVERIFY(qputenv("KODOSI_DATA_ROOT", isolated.toUtf8()));
    QVERIFY(qputenv(
        "KODOSI_PRODUCTION_DATA_ROOT",
        production.toUtf8()));

    auto result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("symlink")));

    QVERIFY(QFile::remove(core));
    QVERIFY(QDir().mkpath(core));
    const auto productionAlias = QDir(directory.path()).filePath(
        QStringLiteral("production-alias"));
    QVERIFY(::symlink(
        QFile::encodeName(core).constData(),
        QFile::encodeName(productionAlias).constData()) == 0);
    QVERIFY(qputenv(
        "KODOSI_PRODUCTION_DATA_ROOT",
        productionAlias.toUtf8()));

    result = kodosi::SingleInstanceGuard::endpointNamespace();
    QVERIFY(!result.valid());
    QVERIFY(result.error.contains(QStringLiteral("production")));
}

void SingleInstanceTest::
    delaysEndpointUntilOwnerReadinessAndDeliversExactlyOnce()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-readiness-XXXXXX")));
    QVERIFY(directory.isValid());
    auto owner = std::make_unique<kodosi::SingleInstanceGuard>();
    const auto ownerResult = owner->start(
        directory.path(),
        QStringLiteral("readiness"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000020")));
    QCOMPARE(
        ownerResult.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    QVERIFY(owner->isOwner());
    QVERIFY(!owner->isReady());
    QVERIFY(!QFileInfo::exists(owner->endpointPath()));
    QSignalSpy routes(
        owner.get(),
        &kodosi::SingleInstanceGuard::routeReceived);

    auto contender = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("readiness"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000021")),
            2000);
    });
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    QCOMPARE(
        contender.wait_for(std::chrono::milliseconds(0)),
        std::future_status::timeout);
    QCOMPARE(routes.count(), 0);

    const auto ready = owner->publishEndpoint();
    QCOMPARE(
        ready.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    QVERIFY2(owner->isReady(), qPrintable(ready.error));
    QTRY_COMPARE_WITH_TIMEOUT(routes.count(), 1, 2000);
    QCOMPARE(
        contender.get().state,
        kodosi::SingleInstanceGuard::StartState::Forwarded);
    QCOMPARE(routes.count(), 1);
    const auto endpoint = owner->endpointPath();
    owner.reset();
    QVERIFY(!QFileInfo::exists(endpoint));
}

void SingleInstanceTest::
    contenderTakesOwnershipOnlyAfterUnreadyOwnerFails()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("kodosi-owner-failure-XXXXXX")));
    QVERIFY(directory.isValid());
    auto owner = std::make_unique<kodosi::SingleInstanceGuard>();
    QCOMPARE(
        owner->start(
            directory.path(),
            QStringLiteral("owner-failure"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000022")))
            .state,
        kodosi::SingleInstanceGuard::StartState::Owner);

    auto contender = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("owner-failure"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000023")),
            2000);
    });
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    QCOMPARE(
        contender.wait_for(std::chrono::milliseconds(0)),
        std::future_status::timeout);

    owner.reset();
    const auto result = contender.get();
    QCOMPARE(
        result.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
}

void SingleInstanceTest::contenderUsesOneAbsoluteReadinessTimeout()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-timeout-XXXXXX")));
    QVERIFY(directory.isValid());
    kodosi::SingleInstanceGuard owner;
    QCOMPARE(
        owner.start(
            directory.path(),
            QStringLiteral("readiness-timeout"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000024")))
            .state,
        kodosi::SingleInstanceGuard::StartState::Owner);

    QElapsedTimer elapsed;
    elapsed.start();
    auto contender = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("readiness-timeout"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000025")),
            150);
    });
    const auto result = contender.get();
    QCOMPARE(
        result.state,
        kodosi::SingleInstanceGuard::StartState::Failed);
    QVERIFY(result.error.contains(QStringLiteral("Timed out")));
    QVERIFY(elapsed.elapsed() >= 100);
    QVERIFY(elapsed.elapsed() < 1000);
    QVERIFY(owner.isOwner());
    QVERIFY(!owner.isReady());
}

void SingleInstanceTest::forwardsAcknowledgesAndDeduplicates()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-XXXXXX")));
    QVERIFY(directory.isValid());
    kodosi::SingleInstanceGuard owner;
    const auto requestId =
        QStringLiteral("01900000-0000-7000-8000-000000000001");
    const auto activation = routeActivation(requestId);
    const auto ownerResult = owner.start(
        directory.path(),
        QStringLiteral("tests"),
        activation);
    QCOMPARE(
        ownerResult.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    const auto ownerReady = owner.publishEndpoint();
    QVERIFY2(
        ownerReady.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ownerReady.error));
    QSignalSpy routes(
        &owner,
        &kodosi::SingleInstanceGuard::routeReceived);

    struct stat status {};
    QVERIFY(::lstat(
        QFile::encodeName(owner.endpointPath()).constData(),
        &status) == 0);
    QVERIFY(S_ISSOCK(status.st_mode));
    QCOMPARE(status.st_uid, ::getuid());
    QCOMPARE(status.st_mode & 0777, static_cast<mode_t>(0600));

    auto first = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("tests"),
            activation);
    });
    QTRY_COMPARE_WITH_TIMEOUT(routes.count(), 1, 3000);
    QCOMPARE(
        first.get().state,
        kodosi::SingleInstanceGuard::StartState::Forwarded);
    QCOMPARE(
        routes.constFirst().constFirst().value<kodosi::DeepLinkDestination>()
            .sessionId,
        QStringLiteral("session-1"));
    QCOMPARE(routes.constFirst().at(1).toString(), QString {});

    auto replay = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("tests"),
            activation);
    });
    QTest::qWait(100);
    QCOMPARE(
        replay.get().state,
        kodosi::SingleInstanceGuard::StartState::Forwarded);
    QCOMPARE(routes.count(), 1);
}

void SingleInstanceTest::forwardsBoundedActivationTokensForEveryKind()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-token-XXXXXX")));
    QVERIFY(directory.isValid());
    const auto token = QString(
        kodosi::SingleInstanceGuard::MaximumActivationTokenBytes,
        u't');
    kodosi::SingleInstanceGuard owner;
    const auto ownerResult = owner.start(
        directory.path(),
        QStringLiteral("tokens"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000010")));
    QCOMPARE(
        ownerResult.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    const auto ownerReady = owner.publishEndpoint();
    QVERIFY2(
        ownerReady.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ownerReady.error));
    QSignalSpy activations(
        &owner,
        &kodosi::SingleInstanceGuard::activationReceived);
    QSignalSpy routes(
        &owner,
        &kodosi::SingleInstanceGuard::routeReceived);
    QSignalSpy invalidRoutes(
        &owner,
        &kodosi::SingleInstanceGuard::invalidRouteReceived);

    const std::vector<kodosi::ApplicationActivation> requests {
        {
            .kind = kodosi::ApplicationActivation::Kind::Activate,
            .requestId =
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000011"),
            .activationToken = token,
        },
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000012"),
            token),
        {
            .kind = kodosi::ApplicationActivation::Kind::InvalidRoute,
            .requestId =
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000013"),
            .parseError =
                kodosi::DeepLinkParseError::MalformedEncoding,
            .activationToken = token,
        },
    };
    for (const auto& request : requests) {
        auto forwarded = std::async(std::launch::async, [&] {
            kodosi::SingleInstanceGuard secondary;
            return secondary.start(
                directory.path(),
                QStringLiteral("tokens"),
                request);
        });
        QTRY_VERIFY_WITH_TIMEOUT(
            activations.count() + routes.count()
                + invalidRoutes.count()
                >= static_cast<int>(&request - requests.data()) + 1,
            3000);
        QCOMPARE(
            forwarded.get().state,
            kodosi::SingleInstanceGuard::StartState::Forwarded);
    }

    QCOMPARE(activations.count(), 1);
    QCOMPARE(activations.constFirst().constFirst().toString(), token);
    QCOMPARE(routes.count(), 1);
    QCOMPARE(routes.constFirst().at(1).toString(), token);
    QCOMPARE(invalidRoutes.count(), 1);
    QCOMPARE(invalidRoutes.constFirst().at(1).toString(), token);
}

void SingleInstanceTest::rejectsUnsafeActivationTokensAndScopesOwnerHook()
{
    EnvironmentGuard activationToken("XDG_ACTIVATION_TOKEN");
    const auto safe = QStringLiteral("activation-token");
    QVERIFY(qputenv("XDG_ACTIVATION_TOKEN", safe.toUtf8()));
    const auto captured =
        kodosi::SingleInstanceGuard::activation(std::nullopt);
    QCOMPARE(captured.activationToken, safe);
    const auto capturedRoute =
        kodosi::SingleInstanceGuard::activation(
            kodosi::DeepLinkRouter::parse(
                u"kodosi://session/session-1"));
    QCOMPARE(capturedRoute.activationToken, safe);
    const auto capturedInvalid =
        kodosi::SingleInstanceGuard::activation(
            kodosi::DeepLinkRouter::parse(
                u"kodosi://session/%GG"));
    QCOMPARE(capturedInvalid.activationToken, safe);

    const auto oversized = QString(
        kodosi::SingleInstanceGuard::MaximumActivationTokenBytes + 1,
        u'x');
    QVERIFY(qputenv("XDG_ACTIVATION_TOKEN", oversized.toUtf8()));
    QVERIFY(
        kodosi::SingleInstanceGuard::activation(std::nullopt)
            .activationToken.isEmpty());
    QVERIFY(qputenv(
        "XDG_ACTIVATION_TOKEN",
        QByteArrayLiteral("unsafe\nvalue")));
    QVERIFY(
        kodosi::SingleInstanceGuard::activation(std::nullopt)
            .activationToken.isEmpty());

    qunsetenv("XDG_ACTIVATION_TOKEN");
    bool observed = false;
    kodosi::SingleInstanceGuard::withActivationToken(
        safe,
        [&] {
            observed =
                qEnvironmentVariable("XDG_ACTIVATION_TOKEN") == safe;
        });
    QVERIFY(observed);
    QVERIFY(!qEnvironmentVariableIsSet("XDG_ACTIVATION_TOKEN"));

    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("si-invalid-token-XXXXXX")));
    QVERIFY(directory.isValid());
    kodosi::SingleInstanceGuard rejected;
    auto invalidActivation = routeActivation(
        QStringLiteral("01900000-0000-7000-8000-000000000014"),
        oversized);
    QCOMPARE(
        rejected.start(
            directory.path(),
            QStringLiteral("invalid"),
            invalidActivation)
            .state,
        kodosi::SingleInstanceGuard::StartState::Failed);
    invalidActivation.activationToken =
        QStringLiteral("unsafe\nvalue");
    QCOMPARE(
        rejected.start(
            directory.path(),
            QStringLiteral("control"),
            invalidActivation)
            .state,
        kodosi::SingleInstanceGuard::StartState::Failed);

    kodosi::SingleInstanceGuard owner;
    const auto ownerStart = owner.start(
        directory.path(),
        QStringLiteral("decode"),
        routeActivation(
            QStringLiteral(
                "01900000-0000-7000-8000-000000000015")));
    QVERIFY2(
        ownerStart.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ownerStart.error));
    const auto ownerReady = owner.publishEndpoint();
    QVERIFY2(
        ownerReady.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ownerReady.error));
    QSignalSpy activations(
        &owner,
        &kodosi::SingleInstanceGuard::activationReceived);
    const QList<QString> unsafeTokens {
        oversized,
        QStringLiteral("unsafe\nvalue"),
        QString(QChar::Null),
    };
    for (auto index = 0; index < unsafeTokens.size(); ++index) {
        const auto unsafeObject = QJsonObject {
            {QStringLiteral("version"), 1},
            {QStringLiteral("requestId"),
             QStringLiteral(
                 "01900000-0000-7000-8000-00000000001%1")
                 .arg(6 + index)},
            {QStringLiteral("kind"), QStringLiteral("activate")},
            {QStringLiteral("activationToken"),
             unsafeTokens.at(index)},
        };
        auto unsafe = std::async(std::launch::async, [&] {
            return sendRaw(
                owner.endpointPath(),
                rawFrame(QJsonDocument(unsafeObject).toJson(
                    QJsonDocument::Compact)));
        });
        QTRY_VERIFY_WITH_TIMEOUT(
            unsafe.wait_for(std::chrono::milliseconds(0))
                == std::future_status::ready,
            3000);
        QVERIFY(unsafe.get());
    }
    QCOMPARE(activations.count(), 0);
}

void SingleInstanceTest::rejectsMalformedAndOversizedFrames()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-malformed-XXXXXX")));
    QVERIFY(directory.isValid());
    kodosi::SingleInstanceGuard owner;
    QVERIFY(owner.start(
        directory.path(),
        QStringLiteral("malformed"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000002")))
        .state == kodosi::SingleInstanceGuard::StartState::Owner);
    QVERIFY(owner.publishEndpoint().state
        == kodosi::SingleInstanceGuard::StartState::Owner);
    QSignalSpy routes(
        &owner,
        &kodosi::SingleInstanceGuard::routeReceived);

    auto malformed = std::async(std::launch::async, [&] {
        return sendRaw(
            owner.endpointPath(),
            rawFrame(QByteArrayLiteral("{not-json")));
    });
    QTRY_VERIFY_WITH_TIMEOUT(
        malformed.wait_for(std::chrono::milliseconds(0))
            == std::future_status::ready,
        3000);
    QVERIFY(malformed.get());

    QByteArray oversized(4, '\0');
    oversized[0] = 0;
    oversized[1] = 1;
    oversized[2] = 0;
    oversized[3] = 0;
    auto tooLarge = std::async(std::launch::async, [&] {
        return sendRaw(owner.endpointPath(), oversized);
    });
    QTRY_VERIFY_WITH_TIMEOUT(
        tooLarge.wait_for(std::chrono::milliseconds(0))
            == std::future_status::ready,
        3000);
    QVERIFY(tooLarge.get());
    QCOMPARE(routes.count(), 0);
}

void SingleInstanceTest::boundsIncompleteAcceptedClients()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-cap-XXXXXX")));
    QVERIFY(directory.isValid());
    kodosi::SingleInstanceGuard owner;
    QVERIFY(owner.start(
        directory.path(),
        QStringLiteral("cap"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000005")))
        .state == kodosi::SingleInstanceGuard::StartState::Owner);
    QVERIFY(owner.publishEndpoint().state
        == kodosi::SingleInstanceGuard::StartState::Owner);

    constexpr auto excessClients = qsizetype {8};
    std::vector<std::unique_ptr<QLocalSocket>> incompleteClients;
    incompleteClients.reserve(
        static_cast<std::size_t>(
            kodosi::SingleInstanceGuard::MaximumActiveClients
            + excessClients));
    for (auto index = qsizetype {0};
         index < kodosi::SingleInstanceGuard::MaximumActiveClients
             + excessClients;
         ++index) {
        auto socket = std::make_unique<QLocalSocket>();
        socket->connectToServer(owner.endpointPath());
        incompleteClients.push_back(std::move(socket));
    }

    QTRY_VERIFY_WITH_TIMEOUT(
        owner.activeClientCount()
            >= kodosi::SingleInstanceGuard::MaximumActiveClients,
        3000);
    QTRY_VERIFY_WITH_TIMEOUT(
        std::ranges::count_if(
            incompleteClients,
            [](const auto& socket) {
                return socket->state()
                    == QLocalSocket::UnconnectedState;
            })
            >= excessClients,
        3000);
    for (auto iteration = 0; iteration < 20; ++iteration) {
        QCoreApplication::processEvents();
        QVERIFY(
            owner.activeClientCount()
            <= kodosi::SingleInstanceGuard::MaximumActiveClients);
        QTest::qWait(5);
    }

    for (const auto& socket : incompleteClients) {
        socket->abort();
    }
    QTRY_COMPARE_WITH_TIMEOUT(owner.activeClientCount(), 0, 3000);

    QSignalSpy routes(
        &owner,
        &kodosi::SingleInstanceGuard::routeReceived);
    auto legitimate = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard secondary;
        return secondary.start(
            directory.path(),
            QStringLiteral("cap"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000006")));
    });
    QTRY_COMPARE_WITH_TIMEOUT(routes.count(), 1, 3000);
    QCOMPARE(
        legitimate.get().state,
        kodosi::SingleInstanceGuard::StartState::Forwarded);
    QTRY_COMPARE_WITH_TIMEOUT(owner.activeClientCount(), 0, 3000);
}

void SingleInstanceTest::shortEndpointNamesFitPortableUnixSocketPaths()
{
    QTemporaryDir directory(
        QStringLiteral("/tmp/")
        + QString(68, u'x')
        + QStringLiteral("-XXXXXX"));
    QVERIFY(directory.isValid());

    kodosi::SingleInstanceGuard owner;
    const auto ownerResult = owner.start(
        directory.path(),
        QStringLiteral("0123456789abcdef"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000030")));
    QCOMPARE(
        ownerResult.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    const auto ready = owner.publishEndpoint();
    QVERIFY2(
        ready.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ready.error));
    const auto nativeEndpoint =
        QFile::encodeName(owner.endpointPath());
    sockaddr_un address {};
    QVERIFY(nativeEndpoint.size()
        < static_cast<qsizetype>(sizeof(address.sun_path)));

    QTemporaryDir oversized(
        QStringLiteral("/tmp/")
        + QString(82, u'x')
        + QStringLiteral("-XXXXXX"));
    QVERIFY(oversized.isValid());
    kodosi::SingleInstanceGuard rejected;
    const auto rejectedResult = rejected.start(
        oversized.path(),
        QStringLiteral("0123456789abcdef"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000031")));
    QCOMPARE(
        rejectedResult.state,
        kodosi::SingleInstanceGuard::StartState::Failed);
    QVERIFY(rejectedResult.error.contains(QStringLiteral("too long")));

    QTemporaryDir multibyte(
        QStringLiteral("/tmp/")
        + QString(42, u'é')
        + QStringLiteral("-XXXXXX"));
    QVERIFY(multibyte.isValid());
    kodosi::SingleInstanceGuard multibyteRejected;
    const auto multibyteResult = multibyteRejected.start(
        multibyte.path(),
        QStringLiteral("0123456789abcdef"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000032")));
    QCOMPARE(
        multibyteResult.state,
        kodosi::SingleInstanceGuard::StartState::Failed);
    QVERIFY(multibyteResult.error.contains(QStringLiteral("too long")));
}

void SingleInstanceTest::removesOnlyUnlockedStaleEndpoint()
{
    QTemporaryDir directory(
        QDir::temp().filePath(
            QStringLiteral("single-instance-stale-XXXXXX")));
    QVERIFY(directory.isValid());
    QVERIFY(::chmod(
        QFile::encodeName(directory.path()).constData(),
        0700) == 0);
    const auto endpoint = QDir(directory.path()).filePath(
        QStringLiteral("stale.s"));
    const auto lockPath = QDir(directory.path()).filePath(
        QStringLiteral("stale.l"));
    QFile lockFile(lockPath);
    QVERIFY(lockFile.open(QIODevice::WriteOnly));
    lockFile.close();
    QVERIFY(QFile::setPermissions(
        lockPath,
        QFileDevice::ReadOwner | QFileDevice::WriteOwner));
    const auto descriptor = ::socket(AF_UNIX, SOCK_STREAM, 0);
    QVERIFY(descriptor >= 0);
    sockaddr_un address {};
    address.sun_family = AF_UNIX;
    const auto nativeEndpoint = QFile::encodeName(endpoint);
    QVERIFY(nativeEndpoint.size()
        < static_cast<qsizetype>(sizeof(address.sun_path)));
    std::copy(
        nativeEndpoint.cbegin(),
        nativeEndpoint.cend(),
        address.sun_path);
    QVERIFY(::bind(
        descriptor,
        reinterpret_cast<const sockaddr*>(&address),
        sizeof(address)) == 0);
    QVERIFY(::chmod(nativeEndpoint.constData(), 0600) == 0);
    ::close(descriptor);

    kodosi::SingleInstanceGuard owner;
    const auto result = owner.start(
        directory.path(),
        QStringLiteral("stale"),
        routeActivation(
            QStringLiteral("01900000-0000-7000-8000-000000000003")));
    QCOMPARE(
        result.state,
        kodosi::SingleInstanceGuard::StartState::Owner);
    const auto ready = owner.publishEndpoint();
    QVERIFY2(
        ready.state
            == kodosi::SingleInstanceGuard::StartState::Owner,
        qPrintable(ready.error));
    QCOMPARE(owner.endpointPath(), endpoint);

    auto secondary = std::async(std::launch::async, [&] {
        kodosi::SingleInstanceGuard contender;
        return contender.start(
            directory.path(),
            QStringLiteral("stale"),
            routeActivation(
                QStringLiteral(
                    "01900000-0000-7000-8000-000000000004")));
    });
    QSignalSpy routes(
        &owner,
        &kodosi::SingleInstanceGuard::routeReceived);
    QTRY_COMPARE_WITH_TIMEOUT(routes.count(), 1, 3000);
    QCOMPARE(
        secondary.get().state,
        kodosi::SingleInstanceGuard::StartState::Forwarded);
}

QTEST_GUILESS_MAIN(SingleInstanceTest)

#include "tst_single_instance.moc"
