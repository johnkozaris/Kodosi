#include "logging/ApplicationLogStore.hpp"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QRandomGenerator>
#include <QRegularExpression>
#include <QThread>
#include <QUrl>

#include <cerrno>
#include <algorithm>
#include <cstdio>
#include <cstring>
#include <map>
#include <shared_mutex>
#include <utility>

#if defined(Q_OS_UNIX)
#include <fcntl.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

Q_LOGGING_CATEGORY(kodosiPerformance, "kodosi.performance")

namespace kodosi {
namespace {

std::shared_mutex messageHandlerMutex;
ApplicationLogStore* activeLogStore = nullptr;
thread_local bool handlingQtMessage = false;

class MessageHandlerGuard final {
public:
    MessageHandlerGuard()
    {
        handlingQtMessage = true;
    }

    ~MessageHandlerGuard()
    {
        handlingQtMessage = false;
    }
};

QString processLogName()
{
    QByteArray randomBytes(16, Qt::Uninitialized);
    for (qsizetype offset = 0; offset < randomBytes.size();
         offset += static_cast<qsizetype>(sizeof(quint32))) {
        const auto value = QRandomGenerator::system()->generate();
        std::memcpy(
            randomBytes.data() + offset,
            &value,
            sizeof(value));
    }
    return QStringLiteral("kodosi-%1-%2.log")
        .arg(QCoreApplication::applicationPid())
        .arg(QString::fromLatin1(randomBytes.toHex()));
}

QString errnoMessage(const char* operation)
{
    return QStringLiteral("%1: %2")
        .arg(QString::fromLatin1(operation), QString::fromLocal8Bit(std::strerror(errno)));
}

QString severityName(const QtMsgType type)
{
    switch (type) {
    case QtDebugMsg:
        return QStringLiteral("debug");
    case QtInfoMsg:
        return QStringLiteral("info");
    case QtWarningMsg:
        return QStringLiteral("warning");
    case QtCriticalMsg:
        return QStringLiteral("critical");
    case QtFatalMsg:
        return QStringLiteral("fatal");
    }
    return QStringLiteral("unknown");
}

QString performanceCategoryName(const PerformanceCategory category)
{
    switch (category) {
    case PerformanceCategory::Terminal:
        return QStringLiteral("terminal");
    }
    return QStringLiteral("unknown");
}

void writeOriginalToStderr(
    const QtMsgType type,
    const QMessageLogContext& context,
    const QString& message)
{
#if defined(Q_OS_UNIX)
    QByteArray line;
    line.reserve(message.size() + 96);
    line += severityName(type).toUtf8();
    if (context.category != nullptr && context.category[0] != '\0') {
        line += " [";
        line += context.category;
        line += ']';
    }
    line += ": ";
    line += message.toUtf8();
    line += '\n';
    qsizetype offset = 0;
    while (offset < line.size()) {
        const auto written = ::write(
            STDERR_FILENO,
            line.constData() + offset,
            static_cast<std::size_t>(line.size() - offset));
        if (written > 0) {
            offset += written;
        } else if (written < 0 && errno == EINTR) {
            continue;
        } else {
            break;
        }
    }
#else
    Q_UNUSED(type);
    Q_UNUSED(context);
    const auto line = message.toLocal8Bit() + '\n';
    std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stderr);
#endif
}

void replaceMatches(
    QString& value,
    const QRegularExpression& expression,
    const QString& replacement,
    const bool preserveFirstCapture = true)
{
    const auto matches = expression.globalMatch(value);
    QList<QRegularExpressionMatch> collected;
    auto iterator = matches;
    while (iterator.hasNext()) {
        collected.append(iterator.next());
    }
    for (auto index = collected.size(); index > 0; --index) {
        const auto& match = collected.at(index - 1);
        value.replace(
            match.capturedStart(),
            match.capturedLength(),
            (preserveFirstCapture ? match.captured(1) : QString {})
                + replacement);
    }
}

void redactUrls(QString& value)
{
    static const QRegularExpression urlPattern(
        QStringLiteral(R"(\b[a-zA-Z][a-zA-Z0-9+.-]*://[^\s<>"']+)"));
    auto iterator = urlPattern.globalMatch(value);
    QList<QRegularExpressionMatch> matches;
    while (iterator.hasNext()) {
        matches.append(iterator.next());
    }
    for (auto index = matches.size(); index > 0; --index) {
        const auto& match = matches.at(index - 1);
        auto encoded = match.captured();
        QString trailing;
        while (!encoded.isEmpty()
               && QStringLiteral(".,);]").contains(encoded.back())) {
            trailing.prepend(encoded.back());
            encoded.chop(1);
        }
        QUrl url(encoded, QUrl::StrictMode);
        if (!url.isValid() || url.scheme().isEmpty()) {
            continue;
        }
        url.setUserName(QString {});
        url.setPassword(QString {});
        url.setQuery(QString {});
        url.setFragment(QString {});
        value.replace(
            match.capturedStart(),
            match.capturedLength(),
            url.toString(QUrl::FullyEncoded) + trailing);
    }
}

void redactNamedSecrets(QString& value)
{
    static const QRegularExpression namedSecretPrefix(
        QStringLiteral(
            R"((["']?(?:access[_-]?token|refresh[_-]?token|api[_-]?key|client[_-]?secret|signing[_-]?key[_-]?pem|key[_-]?material|private[_-]?key|token|secret|password|pem)["']?\s*[:=]\s*))"),
        QRegularExpression::CaseInsensitiveOption);
    const auto replacement = QStringLiteral("[REDACTED]");
    qsizetype searchOffset = 0;
    while (searchOffset < value.size()) {
        const auto match = namedSecretPrefix.match(value, searchOffset);
        if (!match.hasMatch()) {
            break;
        }
        const auto valueStart = match.capturedEnd(1);
        if (valueStart >= value.size()) {
            break;
        }

        auto valueEnd = valueStart;
        const auto quote = value.at(valueStart);
        if (quote == QLatin1Char('"') || quote == QLatin1Char('\'')) {
            valueEnd += 1;
            while (valueEnd < value.size()) {
                if (value.at(valueEnd) == QLatin1Char('\\')) {
                    valueEnd = std::min(valueEnd + 2, value.size());
                } else if (value.at(valueEnd) == quote) {
                    valueEnd += 1;
                    break;
                } else {
                    valueEnd += 1;
                }
            }
        } else {
            while (valueEnd < value.size()
                   && !value.at(valueEnd).isSpace()
                   && !QStringLiteral(",;}").contains(value.at(valueEnd))) {
                valueEnd += 1;
            }
        }
        if (valueEnd == valueStart) {
            searchOffset = valueStart + 1;
            continue;
        }
        value.replace(
            valueStart,
            valueEnd - valueStart,
            replacement);
        searchOffset = valueStart + replacement.size();
    }
}

}

void applicationQtMessageHandler(
    const QtMsgType type,
    const QMessageLogContext& context,
    const QString& message)
{
    if (handlingQtMessage) {
        writeOriginalToStderr(type, context, message);
        return;
    }
    MessageHandlerGuard reentrancyGuard;
    const std::shared_lock lock(messageHandlerMutex);
    auto* store = activeLogStore;
    if (store != nullptr && store->m_previousHandler != nullptr) {
        store->m_previousHandler(type, context, message);
    } else {
        writeOriginalToStderr(type, context, message);
    }
    if (store != nullptr) {
        const auto* categoryBytes =
            context.category == nullptr ? "default" : context.category;
        const auto categoryLength = ::strnlen(
            categoryBytes,
            static_cast<std::size_t>(
                ApplicationLogStore::maximumCategoryCharacters));
        const auto category = QString::fromUtf8(
            categoryBytes,
            static_cast<qsizetype>(categoryLength));
        store->record(type, category, QStringView(message));
    }
}

ApplicationLogStore::ApplicationLogStore(Options options)
    : m_options(std::move(options))
    , m_path(QDir(m_options.directory).filePath(processLogName()))
{
    initialize();
    if (!m_options.installQtMessageHandler) {
        return;
    }
    const std::unique_lock handlerLock(messageHandlerMutex);
    if (activeLogStore != nullptr) {
        const std::scoped_lock stateLock(m_mutex);
        failLocked(QStringLiteral("Another application log store already owns the Qt message handler."));
        return;
    }
    m_previousHandler = qInstallMessageHandler(applicationQtMessageHandler);
    activeLogStore = this;
    m_handlerInstalled = true;
}

ApplicationLogStore::~ApplicationLogStore()
{
    if (m_handlerInstalled) {
        const std::unique_lock handlerLock(messageHandlerMutex);
        if (activeLogStore == this) {
            activeLogStore = nullptr;
            qInstallMessageHandler(m_previousHandler);
        }
    }
    const std::scoped_lock lock(m_mutex);
#if defined(Q_OS_UNIX)
    if (m_fileDescriptor >= 0) {
        (void)::close(m_fileDescriptor);
        m_fileDescriptor = -1;
    }
#endif
}

bool ApplicationLogStore::healthy() const
{
    const std::scoped_lock lock(m_mutex);
    return m_healthy;
}

QString ApplicationLogStore::path() const
{
    return m_path;
}

QString ApplicationLogStore::directory() const
{
    return m_options.directory;
}

qint64 ApplicationLogStore::sizeBytes() const
{
    const std::scoped_lock lock(m_mutex);
    return m_sizeBytes;
}

int ApplicationLogStore::rotationCount() const
{
    const std::scoped_lock lock(m_mutex);
    return m_rotationCount;
}

QString ApplicationLogStore::lastError() const
{
    const std::scoped_lock lock(m_mutex);
    return m_lastError;
}

QString ApplicationLogStore::redact(QString message)
{
    redactUrls(message);

    static const QRegularExpression pemBlock(
        QStringLiteral(
            R"(-----BEGIN ([A-Z0-9][A-Z0-9 -]{0,63})-----[\s\S]*?-----END \1-----)"),
        QRegularExpression::CaseInsensitiveOption);
    replaceMatches(
        message,
        pemBlock,
        QStringLiteral("[REDACTED PEM]"),
        false);
    static const QRegularExpression truncatedPemBlock(
        QStringLiteral(
            R"(-----BEGIN [A-Z0-9][A-Z0-9 -]{0,63}-----[\s\S]*$)"),
        QRegularExpression::CaseInsensitiveOption);
    replaceMatches(
        message,
        truncatedPemBlock,
        QStringLiteral("[REDACTED PEM]"),
        false);

    static const QRegularExpression authorization(
        QStringLiteral(
            R"((\bAuthorization\s*[:=]\s*(?:Bearer|Basic)\s+)[^\s,;]+)"),
        QRegularExpression::CaseInsensitiveOption);
    replaceMatches(message, authorization, QStringLiteral("[REDACTED]"));

    redactNamedSecrets(message);

    static const QRegularExpression jwt(
        QStringLiteral(
            R"((^|[^A-Za-z0-9_-])eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{16,}(?=$|[^A-Za-z0-9_-]))"));
    replaceMatches(message, jwt, QStringLiteral("[REDACTED]"));

    const auto home = QDir::cleanPath(QDir::homePath());
    if (!home.isEmpty() && home != QDir::rootPath()) {
        message.replace(home, QStringLiteral("[HOME]"));
    }

    static const QRegularExpression longMaterial(
        QStringLiteral(
            R"((^|[^A-Za-z0-9])(?:[A-Fa-f0-9]{48,}|[A-Za-z0-9_+/=-]{48,})(?=$|[^A-Za-z0-9]))"));
    replaceMatches(message, longMaterial, QStringLiteral("[REDACTED]"));
    return message;
}

void ApplicationLogStore::record(
    const QtMsgType type,
    QStringView category,
    QStringView message)
{
    const auto messageWasTruncated =
        message.size() > maximumPersistedMessageCharacters;
    auto safeCategory = redact(
        category.first(
            std::min(category.size(), maximumCategoryCharacters))
            .toString());
    auto safeMessage = redact(
        message.first(
            std::min(message.size(), maximumPersistedMessageCharacters))
            .toString());
    if (messageWasTruncated) {
        safeMessage += QStringLiteral(" [TRUNCATED]");
    }

    const auto threadId = reinterpret_cast<quintptr>(QThread::currentThreadId());
    QJsonObject object {
        {QStringLiteral("timestamp"),
         QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)},
        {QStringLiteral("severity"), severityName(type)},
        {QStringLiteral("category"), safeCategory},
        {QStringLiteral("thread"),
         QStringLiteral("0x%1").arg(threadId, 0, 16)},
        {QStringLiteral("message"), safeMessage},
    };
    auto line = QJsonDocument(object).toJson(QJsonDocument::Compact);
    while (line.size() + 1 > m_options.rotationBytes
           && !safeMessage.isEmpty()) {
        const auto excess = line.size() + 1 - m_options.rotationBytes;
        safeMessage.chop(std::min(
            safeMessage.size(),
            static_cast<qsizetype>(excess + 16)));
        if (!safeMessage.endsWith(QStringLiteral(" [TRUNCATED]"))) {
            safeMessage += QStringLiteral(" [TRUNCATED]");
        }
        object.insert(QStringLiteral("message"), safeMessage);
        line = QJsonDocument(object).toJson(QJsonDocument::Compact);
    }
    line += '\n';

    const std::scoped_lock lock(m_mutex);
    if (!m_healthy || m_fileDescriptor < 0) return;
    if (m_sizeBytes > 0 && m_sizeBytes + line.size() > m_options.rotationBytes && !rotate()) return;
    if (m_healthy) (void)writeAll(line);
}

void ApplicationLogStore::initialize()
{
    const std::scoped_lock lock(m_mutex);
    if (m_options.directory.isEmpty()
        || m_options.rotationBytes < 512
        || m_options.rotationBytes > 64 * 1024 * 1024
        || m_options.maximumArchives < 1
        || m_options.maximumArchives > 20
        || m_options.maximumCompletedFamilies < 1
        || m_options.maximumCompletedFamilies > 64) {
        failLocked(QStringLiteral("The application log configuration is invalid."));
        return;
    }
    if (!QDir().mkpath(m_options.directory)) {
        failLocked(QStringLiteral("The application log directory could not be created."));
        return;
    }

#if defined(Q_OS_UNIX)
    const auto nativeDirectory = QFile::encodeName(m_options.directory);
    struct stat directoryStatus {};
    if (::lstat(nativeDirectory.constData(), &directoryStatus) != 0
        || !S_ISDIR(directoryStatus.st_mode)
        || S_ISLNK(directoryStatus.st_mode)) {
        failLocked(QStringLiteral("The application log directory is not a secure directory."));
        return;
    }
    if (::chmod(nativeDirectory.constData(), S_IRWXU) != 0) {
        failLocked(errnoMessage("chmod log directory"));
        return;
    }
#endif
    if (!pruneCompletedFamiliesLocked()
        || !sanitizeCurrentFamilyLocked()) {
        return;
    }
    if (!openCurrentFile()) {
        return;
    }
    if (m_sizeBytes >= m_options.rotationBytes && !rotate()) {
        return;
    }
    refreshArchiveCountLocked();
    m_lastError.clear();
    m_healthy = true;
}

bool ApplicationLogStore::openCurrentFile()
{
#if defined(Q_OS_UNIX)
    const auto nativePath = QFile::encodeName(m_path);
    m_fileDescriptor = ::open(
        nativePath.constData(),
        O_RDWR | O_CREAT | O_APPEND | O_CLOEXEC | O_NOFOLLOW,
        S_IRUSR | S_IWUSR);
    if (m_fileDescriptor < 0) {
        failLocked(errnoMessage("open application log"));
        return false;
    }
    if (::fchmod(m_fileDescriptor, S_IRUSR | S_IWUSR) != 0) {
        failLocked(errnoMessage("chmod application log"));
        (void)::close(m_fileDescriptor);
        m_fileDescriptor = -1;
        return false;
    }
    if (::flock(m_fileDescriptor, LOCK_EX | LOCK_NB) != 0) {
        failLocked(
            errno == EWOULDBLOCK || errno == EAGAIN
                ? QStringLiteral("The application log family is already active.")
                : errnoMessage("lock application log"));
        (void)::close(m_fileDescriptor);
        m_fileDescriptor = -1;
        return false;
    }
    struct stat status {};
    if (::fstat(m_fileDescriptor, &status) != 0 || !S_ISREG(status.st_mode)) {
        failLocked(QStringLiteral("The application log path is not a regular file."));
        (void)::close(m_fileDescriptor);
        m_fileDescriptor = -1;
        return false;
    }
    m_sizeBytes = static_cast<qint64>(status.st_size);
    m_healthy = true;
    return true;
#else
    failLocked(QStringLiteral("Native bounded application logging is unavailable on this platform."));
    return false;
#endif
}

bool ApplicationLogStore::rotate()
{
#if defined(Q_OS_UNIX)
    const auto oldest = m_path + QStringLiteral(".")
        + QString::number(m_options.maximumArchives);
    (void)::unlink(QFile::encodeName(oldest).constData());
    for (auto archive = m_options.maximumArchives - 1; archive >= 1; --archive) {
        const auto source = m_path + QStringLiteral(".") + QString::number(archive);
        const auto destination =
            m_path + QStringLiteral(".") + QString::number(archive + 1);
        const auto nativeSource = QFile::encodeName(source);
        const auto nativeDestination = QFile::encodeName(destination);
        if (::rename(nativeSource.constData(), nativeDestination.constData()) != 0
            && errno != ENOENT) {
            failLocked(errnoMessage("rotate application log archive"));
            return false;
        }
    }
    const auto firstArchive = QFile::encodeName(m_path + QStringLiteral(".1"));
    const auto archiveDescriptor = ::open(
        firstArchive.constData(),
        O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC | O_NOFOLLOW,
        S_IRUSR | S_IWUSR);
    if (archiveDescriptor < 0) {
        failLocked(errnoMessage("create application log archive"));
        return false;
    }
    bool copied = true;
    qint64 offset = 0;
    QByteArray buffer(64 * 1024, Qt::Uninitialized);
    while (offset < m_sizeBytes) {
        const auto requested = static_cast<std::size_t>(
            std::min<qint64>(buffer.size(), m_sizeBytes - offset));
        const auto readCount = ::pread(
            m_fileDescriptor,
            buffer.data(),
            requested,
            static_cast<off_t>(offset));
        if (readCount > 0) {
            ssize_t writtenTotal = 0;
            while (writtenTotal < readCount) {
                const auto written = ::write(
                    archiveDescriptor,
                    buffer.constData() + writtenTotal,
                    static_cast<std::size_t>(readCount - writtenTotal));
                if (written > 0) {
                    writtenTotal += written;
                } else if (written < 0 && errno == EINTR) {
                    continue;
                } else {
                    copied = false;
                    break;
                }
            }
            if (!copied) {
                break;
            }
            offset += readCount;
        } else if (readCount < 0 && errno == EINTR) {
            continue;
        } else {
            copied = false;
            break;
        }
    }
    if (::fchmod(archiveDescriptor, S_IRUSR | S_IWUSR) != 0) {
        copied = false;
    }
    const auto archiveCloseResult = ::close(archiveDescriptor);
    if (!copied || archiveCloseResult != 0) {
        (void)::unlink(firstArchive.constData());
        failLocked(errnoMessage("copy application log archive"));
        return false;
    }
    if (::ftruncate(m_fileDescriptor, 0) != 0) {
        failLocked(errnoMessage("truncate application log after rotation"));
        return false;
    }
    m_sizeBytes = 0;
    refreshArchiveCountLocked();
    return true;
#else
    return false;
#endif
}

bool ApplicationLogStore::writeAll(const QByteArray& bytes)
{
#if defined(Q_OS_UNIX)
    qsizetype offset = 0;
    while (offset < bytes.size()) {
        const auto written = ::write(
            m_fileDescriptor,
            bytes.constData() + offset,
            static_cast<std::size_t>(bytes.size() - offset));
        if (written > 0) {
            offset += written;
            m_sizeBytes += written;
        } else if (written < 0 && errno == EINTR) {
            continue;
        } else {
            failLocked(errnoMessage("write application log"));
            (void)::close(m_fileDescriptor);
            m_fileDescriptor = -1;
            return false;
        }

    }
    return true;
#else
    Q_UNUSED(bytes);
    return false;
#endif
}

bool ApplicationLogStore::sanitizeCurrentFamilyLocked()
{
#if defined(Q_OS_UNIX)
    const auto prefix = QFileInfo(m_path).fileName() + QStringLiteral(".");
    const auto entries = QDir(m_options.directory).entryList(
        {prefix + QStringLiteral("*")},
        QDir::Files | QDir::Dirs | QDir::System | QDir::Hidden,
        QDir::Name);
    for (const auto& entry : entries) {
        bool numberValid = false;
        const auto archiveNumber =
            entry.sliced(prefix.size()).toInt(&numberValid);
        const auto archivePath =
            QDir(m_options.directory).filePath(entry);
        const auto nativeArchive = QFile::encodeName(archivePath);
        struct stat status {};
        if (::lstat(nativeArchive.constData(), &status) != 0) {
            failLocked(errnoMessage("inspect application log archive"));
            return false;
        }

        const auto remove = !numberValid
            || archiveNumber < 1
            || archiveNumber > m_options.maximumArchives
            || !S_ISREG(status.st_mode)
            || S_ISLNK(status.st_mode)
            || status.st_size > m_options.rotationBytes;
        if (remove) {
            if (::unlink(nativeArchive.constData()) != 0) {
                failLocked(errnoMessage("remove invalid application log archive"));
                return false;
            }
            continue;
        }
        if (::chmod(nativeArchive.constData(), S_IRUSR | S_IWUSR) != 0) {
            failLocked(errnoMessage("chmod application log archive"));
            return false;
        }
    }
    return true;
#else
    return false;
#endif
}

bool ApplicationLogStore::pruneCompletedFamiliesLocked()
{
#if defined(Q_OS_UNIX)
    struct FamilyEntry {
        QString path;
        int archiveNumber = 0;
        qint64 size = 0;
        qint64 modification = 0;
    };
    struct Family {
        qint64 newestModification = 0;
        QString basePath;
        std::vector<FamilyEntry> entries;
        int lockDescriptor = -1;
    };

    const QRegularExpression processFilePattern(
        QStringLiteral(
            R"(^kodosi-([1-9][0-9]*)-[0-9a-f]{32}\.log(?:\.([1-9][0-9]*))?$)"));
    std::map<QString, Family> families;
    const auto entries = QDir(m_options.directory).entryInfoList(
        {QStringLiteral("kodosi-*.log*")},
        QDir::Files | QDir::Dirs | QDir::System | QDir::Hidden,
        QDir::Name);
    for (const auto& entry : entries) {
        const auto match = processFilePattern.match(entry.fileName());
        if (!match.hasMatch()) {
            continue;
        }
        const auto archiveCapture = match.captured(2);
        const auto nativePath = QFile::encodeName(entry.filePath());
        struct stat status {};
        if (::lstat(nativePath.constData(), &status) != 0) {
            if (errno == ENOENT) {
                continue;
            }
            failLocked(errnoMessage("inspect completed application log"));
            return false;
        }
        if (!S_ISREG(status.st_mode) || S_ISLNK(status.st_mode)
            || status.st_uid != ::geteuid()) {
            continue;
        }
        const auto familyName = entry.fileName().section(
            QStringLiteral(".log"),
            0,
            0);
        auto& family = families[familyName];
        const auto archiveNumber =
            archiveCapture.isEmpty() ? 0 : archiveCapture.toInt();
        if (archiveNumber == 0) {
            family.basePath = entry.filePath();
        }
        family.entries.push_back({
            .path = entry.filePath(),
            .archiveNumber = archiveNumber,
            .size = static_cast<qint64>(status.st_size),
            .modification = static_cast<qint64>(status.st_mtime),
        });
    }

    std::vector<Family*> ordered;
    ordered.reserve(families.size());
    for (auto& [name, family] : families) {
        Q_UNUSED(name);
        if (!family.basePath.isEmpty()) {
            const auto nativeBase = QFile::encodeName(family.basePath);
            family.lockDescriptor = ::open(
                nativeBase.constData(),
                O_RDWR | O_CLOEXEC | O_NOFOLLOW);
            if (family.lockDescriptor < 0) {
                if (errno == ENOENT) {
                    continue;
                }
                continue;
            }
            if (::flock(family.lockDescriptor, LOCK_EX | LOCK_NB) != 0) {
                const auto locked = errno == EWOULDBLOCK || errno == EAGAIN;
                (void)::close(family.lockDescriptor);
                family.lockDescriptor = -1;
                if (locked) {
                    continue;
                }
                continue;
            }
        }
        std::vector<FamilyEntry> retained;
        retained.reserve(family.entries.size());
        for (const auto& entry : family.entries) {
            const auto nativePath = QFile::encodeName(entry.path);
            if (
                entry.archiveNumber > m_options.maximumArchives
                || entry.size > m_options.rotationBytes
            ) {
                if (::unlink(nativePath.constData()) != 0 && errno != ENOENT) {
                    failLocked(errnoMessage("remove invalid application log file"));
                    if (family.lockDescriptor >= 0) {
                        (void)::close(family.lockDescriptor);
                    }
                    return false;
                }
                continue;
            }
            if (::chmod(nativePath.constData(), S_IRUSR | S_IWUSR) != 0) {
                failLocked(errnoMessage("chmod completed application log file"));
                if (family.lockDescriptor >= 0) {
                    (void)::close(family.lockDescriptor);
                }
                return false;
            }
            family.newestModification = std::max(
                family.newestModification,
                entry.modification);
            retained.push_back(entry);
        }
        family.entries = std::move(retained);
        ordered.push_back(&family);
    }
    std::ranges::sort(
        ordered,
        [](const Family* left, const Family* right) {
            return left->newestModification > right->newestModification;
        });
    for (auto index = static_cast<std::size_t>(
             m_options.maximumCompletedFamilies);
         index < ordered.size();
         ++index) {
        for (const auto& entry : ordered[index]->entries) {
            if (::unlink(QFile::encodeName(entry.path).constData()) != 0
                && errno != ENOENT) {
                failLocked(errnoMessage("prune completed application log family"));
                for (auto* family : ordered) {
                    if (family->lockDescriptor >= 0) {
                        (void)::close(family->lockDescriptor);
                        family->lockDescriptor = -1;
                    }
                }
                return false;
            }
        }
    }
    for (auto* family : ordered) {
        if (family->lockDescriptor >= 0) {
            (void)::close(family->lockDescriptor);
            family->lockDescriptor = -1;
        }
    }
    return true;
#else
    return false;
#endif
}

void ApplicationLogStore::failLocked(QString error)
{
    m_healthy = false;
    m_lastError = std::move(error);
}

void ApplicationLogStore::refreshArchiveCountLocked()
{
    m_rotationCount = 0;
    for (auto archive = 1; archive <= m_options.maximumArchives; ++archive) {
        if (QFile::exists(
                m_path + QStringLiteral(".") + QString::number(archive))) {
            ++m_rotationCount;
        }
    }
}

ScopedPerformanceSpan::ScopedPerformanceSpan(
    const PerformanceCategory category,
    QStringView operation,
    const qint64 thresholdMilliseconds)
    : m_category(category)
    , m_operation(operation.left(64).toString())
    , m_thresholdMilliseconds(std::max<qint64>(0, thresholdMilliseconds))
{
    m_timer.start();
}

ScopedPerformanceSpan::~ScopedPerformanceSpan()
{
    const auto elapsed = m_timer.elapsed();
    if (elapsed < m_thresholdMilliseconds) {
        return;
    }
    qCInfo(kodosiPerformance).noquote()
        << QStringLiteral("category=%1 operation=%2 duration_ms=%3 outcome=%4")
               .arg(
                   performanceCategoryName(m_category),
                   m_operation,
                   QString::number(elapsed),
                   m_outcome);
}

void ScopedPerformanceSpan::setOutcome(QStringView outcome)
{
    m_outcome = outcome.left(32).toString();
}

}
