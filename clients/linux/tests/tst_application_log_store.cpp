#include "logging/ApplicationLogStore.hpp"

#include <QByteArray>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QTest>

#include <atomic>
#include <chrono>
#include <sys/stat.h>
#include <thread>
#include <vector>

namespace {

std::atomic_int forwardedMessages = 0;
std::atomic_int simultaneousHandlers = 0;
std::atomic_int maximumSimultaneousHandlers = 0;

void countingMessageHandler(
    QtMsgType,
    const QMessageLogContext&,
    const QString&)
{
    forwardedMessages.fetch_add(1);
}

void concurrentMessageHandler(
    QtMsgType,
    const QMessageLogContext&,
    const QString&)
{
    const auto active = simultaneousHandlers.fetch_add(1) + 1;
    auto observed = maximumSimultaneousHandlers.load();
    while (active > observed
           && !maximumSimultaneousHandlers.compare_exchange_weak(
               observed,
               active)) {
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    simultaneousHandlers.fetch_sub(1);
}

QTemporaryDir isolatedDirectory(const QString& name)
{
    QDir().mkpath(QDir::current().filePath(QStringLiteral("build")));
    return QTemporaryDir(
        QDir::current().filePath(
            QStringLiteral("build/") + name + QStringLiteral("-XXXXXX")));
}

QByteArray readAll(const QString& path)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return {};
    }
    return file.readAll();
}

kodosi::ApplicationLogStore::Options options(
    const QTemporaryDir& root,
    const qint64 rotationBytes = 2 * 1024 * 1024,
    const int archives = 5,
    const bool installHandler = false)
{
    return {
        .directory = QDir(root.path()).filePath(QStringLiteral("state/logs")),
        .rotationBytes = rotationBytes,
        .maximumArchives = archives,
        .maximumCompletedFamilies = 2,
        .installQtMessageHandler = installHandler,
    };
}

}

class ApplicationLogStoreTest final : public QObject {
    Q_OBJECT

private slots:
    void createsPrivateFilesAndRedactsStructuredLines();
    void redactsCompleteQuotedNamedSecrets();
    void truncatesBeforeRegexWithoutLeakingPartialPem();
    void rotatesAndRetainsOnlyTheConfiguredArchives();
    void concurrentStoresRotateIndependentProcessFamilies();
    void startupPrunesByFamilyLockRatherThanPid();
    void serializesConcurrentWriters();
    void restoresThePreviousQtMessageHandler();
    void handlerAllowsConcurrentPreviousHandlerCalls();
    void scopedPerformanceSpansPersistOnlyBoundedMetadata();
    void reportsWritePathFailuresWithoutRecursion();
    void keepsExplicitRootsIsolated();
};

void ApplicationLogStoreTest::createsPrivateFilesAndRedactsStructuredLines()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-private"));
    QVERIFY(root.isValid());
    kodosi::ApplicationLogStore store(options(root));
    QVERIFY2(store.healthy(), qPrintable(store.lastError()));

    const auto base64Options =
        QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals;
    const auto fakeJwt = QString::fromLatin1(
        QByteArrayLiteral(R"({"alg":"HS256"})").toBase64(base64Options)
        + '.'
        + QByteArrayLiteral(R"({"sub":"secret-user"})").toBase64(base64Options)
        + '.'
        + QByteArrayLiteral("abcdefghijklmnopqrstuvwxyz012345"));
    const auto fakePrivateKey = QStringLiteral(
        "-----BEGIN %1-----\nshort\nwrapped\n-----END %1-----")
        .arg(QStringLiteral("PRIVATE KEY"));

    store.record(
        QtWarningMsg,
        QStringLiteral("kodosi.test"),
        QStringLiteral(
            "Authorization: Bearer bearer-secret "
            "password=hunter2 "
            "{\"privateKey\":\"private-material\"} "
            "%1 \"%2\" "
            "{\"apiKey\":\"api-material\","
            "\"clientSecret\":\"client-material\","
            "\"signingKeyPem\":\"signing-material\","
            "\"pem\":\"pem-material\","
            "\"keyMaterial\":\"key-material\"} "
            "password=hunter2 "
            "privateKey=private-material "
            "https://alice:password@example.com/path?token=query#fragment "
            "https://alice:pw@example.com/path?token=query#fragment ")
            .arg(fakeJwt, fakePrivateKey)
            + QDir::homePath()
            + QStringLiteral(
                "/private "
                "0123456789abcdef0123456789abcdef0123456789abcdef"));

    struct stat directoryStatus {};
    struct stat fileStatus {};
    QVERIFY(::stat(QFile::encodeName(store.directory()).constData(), &directoryStatus) == 0);
    QVERIFY(::stat(QFile::encodeName(store.path()).constData(), &fileStatus) == 0);
    QCOMPARE(directoryStatus.st_mode & 0777, static_cast<mode_t>(0700));
    QCOMPARE(fileStatus.st_mode & 0777, static_cast<mode_t>(0600));

    const auto line = readAll(store.path()).trimmed();
    const auto document = QJsonDocument::fromJson(line);
    QVERIFY(document.isObject());
    const auto object = document.object();
    QCOMPARE(object.value(QStringLiteral("severity")).toString(), QStringLiteral("warning"));
    QCOMPARE(object.value(QStringLiteral("category")).toString(), QStringLiteral("kodosi.test"));
    QVERIFY(object.value(QStringLiteral("timestamp")).toString().endsWith(QLatin1Char('Z')));
    QVERIFY(object.value(QStringLiteral("thread")).toString().startsWith(QStringLiteral("0x")));
    const auto message = object.value(QStringLiteral("message")).toString();
    QVERIFY(message.contains(QStringLiteral("[REDACTED]")));
    QVERIFY(message.contains(QStringLiteral("[HOME]")));
    QVERIFY(message.contains(QStringLiteral("https://example.com/path")));
    QVERIFY(!message.contains(QStringLiteral("bearer-secret")));
    QVERIFY(!message.contains(QStringLiteral("hunter2")));
    QVERIFY(!message.contains(QStringLiteral("private-material")));
    QVERIFY(!message.contains(QStringLiteral("eyJhbGci")));
    QVERIFY(!message.contains(QStringLiteral("secret-key-body")));
    QVERIFY(!message.contains(QStringLiteral("api-material")));
    QVERIFY(!message.contains(QStringLiteral("client-material")));
    QVERIFY(!message.contains(QStringLiteral("signing-material")));
    QVERIFY(!message.contains(QStringLiteral("pem-material")));
    QVERIFY(!message.contains(QStringLiteral("key-material")));
    QVERIFY(!message.contains(QStringLiteral("short")));
    QVERIFY(!message.contains(QStringLiteral("wrapped")));
    QVERIFY(!message.contains(QStringLiteral("alice")));
    QVERIFY(!message.contains(QStringLiteral("query")));
    QVERIFY(!message.contains(QStringLiteral("fragment")));
    QVERIFY(!message.contains(QDir::homePath()));
}

void ApplicationLogStoreTest::redactsCompleteQuotedNamedSecrets()
{
    const QStringList keys {
        QStringLiteral("password"),
        QStringLiteral("token"),
        QStringLiteral("apiKey"),
        QStringLiteral("clientSecret"),
        QStringLiteral("privateKey"),
        QStringLiteral("pem"),
        QStringLiteral("keyMaterial"),
    };
    for (qsizetype index = 0; index < keys.size(); ++index) {
        const auto& key = keys.at(index);
        const auto input = index % 2 == 0
            ? QStringLiteral(
                  R"json({"%1":"prefix\"quote\\backslash\u005ctiny-suffix","safe":"visible"})json")
                  .arg(key)
            : QStringLiteral(
                  R"kv(%1='prefix\'quote\\backslash\u005ctiny-suffix' safe=visible)kv")
                  .arg(key);
        const auto expected = index % 2 == 0
            ? QStringLiteral(R"json({"%1":[REDACTED],"safe":"visible"})json")
                  .arg(key)
            : QStringLiteral("%1=[REDACTED] safe=visible").arg(key);
        QCOMPARE(kodosi::ApplicationLogStore::redact(input), expected);
    }

    QCOMPARE(
        kodosi::ApplicationLogStore::redact(
            QStringLiteral(R"kv(token="prefix\"quote, short leaked suffix)kv")),
        QStringLiteral("token=[REDACTED]"));
    QCOMPARE(
        kodosi::ApplicationLogStore::redact(
            QStringLiteral(R"kv(apiKey='prefix\'quote; short leaked suffix)kv")),
        QStringLiteral("apiKey=[REDACTED]"));
    QCOMPARE(
        kodosi::ApplicationLogStore::redact(
            QStringLiteral("password=secret safe=value")),
        QStringLiteral("password=[REDACTED] safe=value"));
}

void ApplicationLogStoreTest::truncatesBeforeRegexWithoutLeakingPartialPem()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-bounded-redaction"));
    QVERIFY(root.isValid());
    kodosi::ApplicationLogStore store(options(root));
    QVERIFY(store.healthy());
    const auto largeSecret =
        QStringLiteral("pem=-----BEGIN PRIVATE KEY-----\n")
        + QString(1024 * 1024, QLatin1Char('s'));
    store.record(
        QtWarningMsg,
        QStringLiteral("kodosi.bounded-redaction"),
        largeSecret);
    const auto document =
        QJsonDocument::fromJson(readAll(store.path()).trimmed());
    QVERIFY(document.isObject());
    const auto message =
        document.object().value(QStringLiteral("message")).toString();
    QVERIFY(message.contains(QStringLiteral("[REDACTED]")));
    QVERIFY(message.endsWith(QStringLiteral("[TRUNCATED]")));
    QVERIFY(!message.contains(QString(64, QLatin1Char('s'))));
    QVERIFY(message.size() < 1024);
}

void ApplicationLogStoreTest::rotatesAndRetainsOnlyTheConfiguredArchives()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-rotation"));
    QVERIFY(root.isValid());
    const auto logDirectory =
        QDir(root.path()).filePath(QStringLiteral("state/logs"));
    QVERIFY(QDir().mkpath(logDirectory));
    kodosi::ApplicationLogStore store(options(root, 1024, 5));
    QVERIFY(store.healthy());
    for (auto index = 0; index < 300; ++index) {
        store.record(
            QtInfoMsg,
            QStringLiteral("kodosi.rotation"),
            QStringLiteral("bounded rotation record %1 %2")
                .arg(index)
                .arg(QString(80, QLatin1Char('x'))));
    }
    QCOMPARE(store.rotationCount(), 5);
    for (auto archive = 1; archive <= 5; ++archive) {
        QVERIFY(QFile::exists(
            store.path() + QStringLiteral(".") + QString::number(archive)));
    }
    QVERIFY(!QFile::exists(store.path() + QStringLiteral(".6")));
    QVERIFY(store.sizeBytes() <= 1024);
}

void ApplicationLogStoreTest::concurrentStoresRotateIndependentProcessFamilies()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-multi-instance"));
    QVERIFY(root.isValid());
    kodosi::ApplicationLogStore first(options(root, 1024, 2));
    kodosi::ApplicationLogStore second(options(root, 1024, 2));
    QVERIFY(first.healthy());
    QVERIFY(second.healthy());
    QVERIFY(first.path() != second.path());

    std::thread firstWriter([&first] {
        for (auto index = 0; index < 200; ++index) {
            first.record(
                QtInfoMsg,
                QStringLiteral("kodosi.first"),
                QStringLiteral("first-family-%1-%2")
                    .arg(index)
                    .arg(QString(80, QLatin1Char('a'))));
        }
    });
    std::thread secondWriter([&second] {
        for (auto index = 0; index < 200; ++index) {
            second.record(
                QtInfoMsg,
                QStringLiteral("kodosi.second"),
                QStringLiteral("second-family-%1-%2")
                    .arg(index)
                    .arg(QString(80, QLatin1Char('b'))));
        }
    });
    firstWriter.join();
    secondWriter.join();

    QCOMPARE(first.rotationCount(), 2);
    QCOMPARE(second.rotationCount(), 2);
    for (auto archive = 1; archive <= 2; ++archive) {
        QVERIFY(QFile::exists(
            first.path() + QStringLiteral(".") + QString::number(archive)));
        QVERIFY(QFile::exists(
            second.path() + QStringLiteral(".") + QString::number(archive)));
    }
    QVERIFY(!readAll(first.path()).contains("second-family"));
    QVERIFY(!readAll(second.path()).contains("first-family"));
}

void ApplicationLogStoreTest::startupPrunesByFamilyLockRatherThanPid()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-pruning"));
    QVERIFY(root.isValid());
    const auto logDirectory =
        QDir(root.path()).filePath(QStringLiteral("state/logs"));
    QVERIFY(QDir().mkpath(logDirectory));

    kodosi::ApplicationLogStore first(options(root, 1024, 2));
    kodosi::ApplicationLogStore second(options(root, 1024, 2));
    QVERIFY(first.healthy());
    QVERIFY(second.healthy());
    QVERIFY(first.path() != second.path());

    QStringList staleBases;
    for (auto family = 0; family < 4; ++family) {
        const auto suffix = QStringLiteral("%1").arg(
            family + 1,
            32,
            16,
            QLatin1Char('0'));
        const auto base = QDir(logDirectory).filePath(
            QStringLiteral("kodosi-%1-%2.log")
                .arg(QCoreApplication::applicationPid())
                .arg(suffix));
        staleBases.append(base);
        QFile file(base);
        QVERIFY(file.open(QIODevice::WriteOnly));
        QVERIFY(file.write("stale") == 5);
        file.close();
        QVERIFY(file.open(QIODevice::ReadOnly));
        QVERIFY(file.setFileTime(
            QDateTime::fromSecsSinceEpoch(100 + family),
            QFileDevice::FileModificationTime));
        file.close();
    }

    kodosi::ApplicationLogStore third(options(root, 1024, 2));
    QVERIFY(third.healthy());
    QVERIFY(QFile::exists(first.path()));
    QVERIFY(QFile::exists(second.path()));
    QVERIFY(!QFile::exists(staleBases.at(0)));
    QVERIFY(!QFile::exists(staleBases.at(1)));
    QVERIFY(QFile::exists(staleBases.at(2)));
    QVERIFY(QFile::exists(staleBases.at(3)));
}

void ApplicationLogStoreTest::serializesConcurrentWriters()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-concurrent"));
    QVERIFY(root.isValid());
    kodosi::ApplicationLogStore store(options(root, 2 * 1024 * 1024, 5));
    QVERIFY(store.healthy());
    std::vector<std::thread> writers;
    for (auto writer = 0; writer < 8; ++writer) {
        writers.emplace_back([&store, writer] {
            for (auto line = 0; line < 100; ++line) {
                store.record(
                    QtDebugMsg,
                    QStringLiteral("kodosi.concurrent"),
                    QStringLiteral("writer=%1 line=%2").arg(writer).arg(line));
            }
        });
    }
    for (auto& writer : writers) {
        writer.join();
    }
    const auto lines = readAll(store.path()).split('\n');
    QCOMPARE(lines.size() - 1, 800);
    for (auto index = 0; index < lines.size() - 1; ++index) {
        QVERIFY(QJsonDocument::fromJson(lines.at(index)).isObject());
    }
}

void ApplicationLogStoreTest::restoresThePreviousQtMessageHandler()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-handler"));
    QVERIFY(root.isValid());
    forwardedMessages.store(0);
    const auto original = qInstallMessageHandler(countingMessageHandler);
    {
        kodosi::ApplicationLogStore store(options(root, 4096, 2, true));
        QVERIFY(store.healthy());
        qWarning().noquote() << "forwarded and persisted";
    }
    qWarning().noquote() << "forwarded after destruction";
    const auto restored = qInstallMessageHandler(original);
    QVERIFY(restored == countingMessageHandler);
    QCOMPARE(forwardedMessages.load(), 2);
}

void ApplicationLogStoreTest::handlerAllowsConcurrentPreviousHandlerCalls()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-shared-handler"));
    QVERIFY(root.isValid());
    simultaneousHandlers.store(0);
    maximumSimultaneousHandlers.store(0);
    const auto original = qInstallMessageHandler(concurrentMessageHandler);
    {
        kodosi::ApplicationLogStore store(options(root, 64 * 1024, 2, true));
        QVERIFY(store.healthy());
        std::vector<std::thread> writers;
        for (auto index = 0; index < 6; ++index) {
            writers.emplace_back([index] {
                qWarning().noquote()
                    << QStringLiteral("parallel-handler-%1").arg(index);
            });
        }
        for (auto& writer : writers) {
            writer.join();
        }
    }
    (void)qInstallMessageHandler(original);
    QVERIFY(maximumSimultaneousHandlers.load() > 1);
}

void ApplicationLogStoreTest::
scopedPerformanceSpansPersistOnlyBoundedMetadata()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-performance"));
    QVERIFY(root.isValid());
    kodosi::ApplicationLogStore store(options(root, 4096, 2, true));
    QVERIFY(store.healthy());
    {
        kodosi::ScopedPerformanceSpan span(
            kodosi::PerformanceCategory::Terminal,
            QStringLiteral("frame.raster"),
            0);
        span.setOutcome(QStringLiteral("ok"));
    }
    const auto contents = readAll(store.path());
    QVERIFY(contents.contains("category=terminal"));
    QVERIFY(contents.contains("operation=frame.raster"));
    QVERIFY(contents.contains("duration_ms="));
    QVERIFY(contents.contains("outcome=ok"));
    QVERIFY(!contents.contains("payload"));
    QVERIFY(!contents.contains("session"));
}

void ApplicationLogStoreTest::reportsWritePathFailuresWithoutRecursion()
{
    auto root = isolatedDirectory(QStringLiteral("application-log-failure"));
    QVERIFY(root.isValid());
    const auto logDirectory =
        QDir(root.path()).filePath(QStringLiteral("state/logs"));
    QVERIFY(QDir().mkpath(QFileInfo(logDirectory).dir().absolutePath()));
    QFile blockingPath(logDirectory);
    QVERIFY(blockingPath.open(QIODevice::WriteOnly));
    blockingPath.close();
    kodosi::ApplicationLogStore store({
        .directory = logDirectory,
        .installQtMessageHandler = false,
    });
    QVERIFY(!store.healthy());
    QVERIFY(!store.lastError().isEmpty());
    store.record(
        QtCriticalMsg,
        QStringLiteral("kodosi.failure"),
        QStringLiteral("must not recurse"));
}

void ApplicationLogStoreTest::keepsExplicitRootsIsolated()
{
    auto firstRoot = isolatedDirectory(QStringLiteral("application-log-first"));
    auto secondRoot = isolatedDirectory(QStringLiteral("application-log-second"));
    QVERIFY(firstRoot.isValid());
    QVERIFY(secondRoot.isValid());
    kodosi::ApplicationLogStore first(options(firstRoot));
    kodosi::ApplicationLogStore second(options(secondRoot));
    first.record(
        QtInfoMsg,
        QStringLiteral("kodosi.isolation"),
        QStringLiteral("first-root-only"));
    second.record(
        QtInfoMsg,
        QStringLiteral("kodosi.isolation"),
        QStringLiteral("second-root-only"));
    QVERIFY(readAll(first.path()).contains("first-root-only"));
    QVERIFY(!readAll(first.path()).contains("second-root-only"));
    QVERIFY(readAll(second.path()).contains("second-root-only"));
    QVERIFY(first.path().startsWith(firstRoot.path()));
    QVERIFY(second.path().startsWith(secondRoot.path()));
}

QTEST_APPLESS_MAIN(ApplicationLogStoreTest)

#include "tst_application_log_store.moc"
