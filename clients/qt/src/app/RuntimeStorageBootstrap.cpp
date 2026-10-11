#include "app/RuntimeStorageBootstrap.hpp"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QStandardPaths>

#include <cerrno>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

namespace kodosi {
namespace {

bool within(const QString& path, const QString& root)
{
    return path == root || path.startsWith(root + QLatin1Char('/'));
}

std::expected<QString, QString> normalized(const QString& path)
{
    if (path.isEmpty() || !QDir::isAbsolutePath(path) || path.contains(QLatin1Char('$'))
        || path.split(QLatin1Char('/'), Qt::SkipEmptyParts).contains(QStringLiteral("..")))
        return std::unexpected(QStringLiteral("Storage paths must be absolute without variables or parent-directory components."));
    for (const auto character : path) {
        if (character.isNull() || character.category() == QChar::Other_Control)
            return std::unexpected(QStringLiteral("Storage paths must not contain control characters."));
    }
    const auto clean = QDir::cleanPath(path);
    if (clean == QStringLiteral("/"))
        return std::unexpected(QStringLiteral("Storage must not use the filesystem root."));
    return clean;
}

std::expected<QString, QString> resolvedAncestor(QString path)
{
    QStringList missing;
    while (true) {
        const QFileInfo info(path);
        if (info.exists()) {
            auto canonical = info.canonicalFilePath();
            if (canonical.isEmpty()) break;
            for (auto it = missing.crbegin(); it != missing.crend(); ++it)
                canonical = QDir(canonical).filePath(*it);
            return canonical;
        }
        if (info.isSymLink() || path == QStringLiteral("/")) break;
        missing.append(info.fileName());
        path = info.dir().absolutePath();
    }
    return std::unexpected(QStringLiteral("Storage paths could not be resolved safely."));
}

std::expected<bool, QString> aliasesDirectory(const QString& path, const QString& protectedPath)
{
    struct stat protectedStatus {};
    if (::stat(QFile::encodeName(protectedPath).constData(), &protectedStatus) != 0) {
        if (errno == ENOENT) return false;
        return std::unexpected(QStringLiteral("Protected storage cannot be inspected."));
    }
    QString current = QStringLiteral("/");
    for (const auto& part : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
        current = QDir(current).filePath(part);
        struct stat status {};
        if (::stat(QFile::encodeName(current).constData(), &status) != 0) {
            if (errno == ENOENT) return false;
            return std::unexpected(QStringLiteral("Storage aliases cannot be inspected."));
        }
        if (status.st_dev == protectedStatus.st_dev && status.st_ino == protectedStatus.st_ino)
            return true;
    }
    return false;
}

std::expected<void, QString> validateDirectory(const QString& path)
{
    QString current = QStringLiteral("/");
    for (const auto& part : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
        current = QDir(current).filePath(part);
        struct stat status {};
        if (::lstat(QFile::encodeName(current).constData(), &status) != 0) {
            if (errno == ENOENT) return {};
            return std::unexpected(QStringLiteral("A storage directory cannot be inspected."));
        }
        if (!S_ISDIR(status.st_mode) || S_ISLNK(status.st_mode))
            return std::unexpected(QStringLiteral("Storage directories must not traverse symlinks or files."));
        if (current == path && status.st_uid != ::geteuid())
            return std::unexpected(QStringLiteral("Storage belongs to another user."));
    }
    return {};
}

std::expected<void, QString> secureDirectory(const QString& path)
{
    int descriptor = ::open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (descriptor < 0)
        return std::unexpected(QStringLiteral("Storage root cannot be opened."));
    for (const auto& part : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
        const auto name = QFile::encodeName(part);
        if (::mkdirat(descriptor, name.constData(), 0700) != 0 && errno != EEXIST) {
            ::close(descriptor);
            return std::unexpected(QStringLiteral("Private storage cannot be created."));
        }
        const int next = ::openat(descriptor, name.constData(), O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
        ::close(descriptor);
        descriptor = next;
        if (descriptor < 0)
            return std::unexpected(QStringLiteral("Private storage traverses an unsafe directory."));
    }
    struct stat status {};
    const bool valid = ::fstat(descriptor, &status) == 0 && status.st_uid == ::geteuid()
        && ::fchmod(descriptor, 0700) == 0;
    ::close(descriptor);
    if (!valid)
        return std::unexpected(QStringLiteral("Private storage ownership or permissions are invalid."));
    return {};
}

std::expected<void, QString> validatePreferences(const QString& path)
{
    struct stat status {};
    if (::lstat(QFile::encodeName(path).constData(), &status) != 0) {
        if (errno == ENOENT) return {};
        return std::unexpected(QStringLiteral("Isolated preferences cannot be inspected."));
    }
    if (!S_ISREG(status.st_mode) || status.st_uid != ::geteuid() || status.st_nlink != 1)
        return std::unexpected(QStringLiteral("Isolated preferences must be a private regular file."));
    return {};
}

}

std::expected<RuntimeStorageBootstrap, QString> RuntimeStorageBootstrap::resolve()
{
    const auto production = normalized(qEnvironmentVariableIsSet("KODOSI_PRODUCTION_DATA_ROOT")
        ? qEnvironmentVariable("KODOSI_PRODUCTION_DATA_ROOT")
        : QDir(QStandardPaths::writableLocation(QStandardPaths::ConfigLocation)).filePath(QStringLiteral("kodosi")));
    if (!production) return std::unexpected(production.error());
    RuntimeStorageBootstrap result;
    result.runtimeRoot = *production;
    result.logDirectory = QDir(QStandardPaths::writableLocation(QStandardPaths::StateLocation)).filePath(QStringLiteral("logs"));
    if (qEnvironmentVariableIsSet("KODOSI_DATA_ROOT")) {
        if (!qEnvironmentVariableIsSet("KODOSI_PRODUCTION_DATA_ROOT"))
            return std::unexpected(QStringLiteral("KODOSI_PRODUCTION_DATA_ROOT is required with KODOSI_DATA_ROOT."));
        const auto isolated = normalized(qEnvironmentVariable("KODOSI_DATA_ROOT"));
        const auto home = normalized(QDir::homePath());
        if (!isolated) return std::unexpected(isolated.error());
        if (!home) return std::unexpected(home.error());
        const auto canonicalIsolated = resolvedAncestor(*isolated);
        const auto canonicalProduction = resolvedAncestor(*production);
        const auto canonicalHome = resolvedAncestor(*home);
        if (!canonicalIsolated || !canonicalProduction || !canonicalHome)
            return std::unexpected(QStringLiteral("Isolated and production storage could not be compared."));
        const auto aliasesProduction = aliasesDirectory(*isolated, *production);
        const auto containsProduction = aliasesDirectory(*production, *isolated);
        if (!aliasesProduction || !containsProduction)
            return std::unexpected(QStringLiteral("Isolated and production aliases could not be compared."));
        if (*aliasesProduction || *containsProduction
            || within(*home, *isolated) || within(*isolated, *production) || within(*production, *isolated)
            || within(*canonicalHome, *canonicalIsolated)
            || within(*canonicalIsolated, *canonicalProduction) || within(*canonicalProduction, *canonicalIsolated))
            return std::unexpected(QStringLiteral("Isolated storage must not overlap home or production storage."));
        result.isolatedRoot = *isolated;
        result.runtimeRoot = QDir(*isolated).filePath(QStringLiteral("core"));
        const auto native = QDir(*isolated).filePath(QStringLiteral("qt"));
        result.preferencesPath = QDir(native).filePath(QStringLiteral("preferences.ini"));
        result.logDirectory = QDir(native).filePath(QStringLiteral("logs"));
        if (auto valid = validateDirectory(*isolated); !valid) return std::unexpected(valid.error());
        if (auto valid = validateDirectory(native); !valid) return std::unexpected(valid.error());
        if (auto valid = validateDirectory(result.logDirectory); !valid) return std::unexpected(valid.error());
        if (auto valid = validatePreferences(result.preferencesPath); !valid) return std::unexpected(valid.error());
    }
    if (auto valid = validateDirectory(result.runtimeRoot); !valid) return std::unexpected(valid.error());
    return result;
}

std::expected<void, QString> RuntimeStorageBootstrap::prepare() const
{
    const auto current = resolve();
    if (!current || current->runtimeRoot != runtimeRoot || current->preferencesPath != preferencesPath
        || current->logDirectory != logDirectory)
        return std::unexpected(QStringLiteral("Storage changed before initialization."));
    if (isolatedRoot.isEmpty()) return {};
    for (const auto& path : {isolatedRoot, QFileInfo(preferencesPath).absolutePath(), logDirectory}) {
        if (auto result = secureDirectory(path); !result) return result;
    }
    const auto native = QFile::encodeName(preferencesPath);
    const int descriptor = ::open(native.constData(), O_WRONLY | O_CREAT | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (descriptor < 0) return std::unexpected(QStringLiteral("Isolated preferences could not be opened."));
    struct stat status {};
    const bool valid = ::fstat(descriptor, &status) == 0 && S_ISREG(status.st_mode)
        && status.st_uid == ::geteuid() && status.st_nlink == 1 && ::fchmod(descriptor, 0600) == 0;
    ::close(descriptor);
    if (!valid) return std::unexpected(QStringLiteral("Isolated preference ownership or permissions are invalid."));
    return {};
}

std::unique_ptr<QSettings> RuntimeStorageBootstrap::settings() const
{
    if (preferencesPath.isEmpty()) return std::make_unique<QSettings>();
    auto settings = std::make_unique<QSettings>(preferencesPath, QSettings::IniFormat);
    settings->setFallbacksEnabled(false);
    return settings;
}

}
