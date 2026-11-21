#include "platform/DesktopFileIntegration.hpp"
#include <QClipboard>
#include <QDesktopServices>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QGuiApplication>
#include <QSaveFile>
#include <QStandardPaths>
#include <QWindow>
#ifdef Q_OS_WIN
#include <QProcess>
#else
#include <QDBusConnection>
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDebug>
#endif

namespace kodosi {
DesktopFileIntegration::DesktopFileIntegration(QObject* parent)
    : DesktopFileIntegration([](const QUrl& url) { return QDesktopServices::openUrl(url); }, parent)
{
}
DesktopFileIntegration::DesktopFileIntegration(UrlOpener opener, QObject* parent)
    : QObject(parent)
    , m_opener(std::move(opener))
{
}
DesktopFileIntegration::~DesktopFileIntegration()
{
    if (m_dialog)
        delete m_dialog.data();
}
bool DesktopFileIntegration::busy() const
{
    return !m_dialog.isNull();
}
void DesktopFileIntegration::setTransientParent(QWindow* window)
{
    m_window = window;
}
QString DesktopFileIntegration::canonicalLocalPath(const QString& path, bool directoryOnly)
{
    // A Windows drive letter parses as a one-letter URL scheme.
    if (path.isEmpty() || path.size() > 4096 || path.contains(QChar::Null) || !QDir::isAbsolutePath(path)
        || QUrl(path).scheme().size() > 1)
        return {};
    const QFileInfo info(path);
    if (!info.exists() || !info.isReadable() || (directoryOnly && !info.isDir())
        || (!info.isDir() && !info.isFile()))
        return {};
    const auto canonical = info.canonicalFilePath();
    if (canonical.isEmpty() || (info.isDir() && !info.isExecutable()))
        return {};
    return canonical;
}
bool DesktopFileIntegration::requestDirectory(const QString& purpose, const QString& initial)
{
    if (m_dialog) {
        fail(tr("A folder picker is already open."));
        return false;
    }
    if (purpose.isEmpty() || purpose.size() > 128)
        return false;
    clearError();
    auto* dialog = new QFileDialog;
    m_dialog = dialog;
    dialog->setWindowTitle(tr("Choose a folder"));
    dialog->setAccessibleName(dialog->windowTitle());
    dialog->setFileMode(QFileDialog::Directory);
    dialog->setOption(QFileDialog::ShowDirsOnly, true);
    dialog->setSupportedSchemes({ QStringLiteral("file") });
    dialog->setWindowModality(Qt::ApplicationModal);
    const auto path = canonicalLocalPath(initial, true);
    dialog->setDirectory(path.isEmpty() ? QDir::homePath() : path);
    connect(dialog, &QDialog::finished, this, [this, dialog, purpose](int result) {
        m_dialog.clear();
        emit busyChanged();
        if (result == QDialog::Accepted) {
            const auto urls = dialog->selectedUrls();
            const auto selected = urls.size() == 1 && urls.first().isLocalFile()
                ? canonicalLocalPath(urls.first().toLocalFile(), true)
                : QString {};
            if (selected.isEmpty())
                fail(tr("Choose a readable local folder."));
            else
                emit directoryPicked(purpose, selected);
        }
        dialog->deleteLater();
    });
    dialog->setAttribute(Qt::WA_NativeWindow);
    if (dialog->windowHandle() && m_window)
        dialog->windowHandle()->setTransientParent(m_window);
    dialog->show();
    emit busyChanged();
    return true;
}
bool DesktopFileIntegration::requestFiles(const QString& purpose)
{
    return pickFiles(tr("Add files to the canvas"), true, [this, purpose](const QList<QUrl>& files) {
        if (!files.isEmpty())
            emit filesPicked(purpose, files);
    });
}

bool DesktopFileIntegration::requestPageFiles(const QString& sessionId, const bool several)
{
    return pickFiles(tr("Choose files for this page"), several,
        [this, sessionId](const QList<QUrl>& files) { emit pageFilesPicked(sessionId, files); });
}

bool DesktopFileIntegration::pickFiles(const QString& title, const bool several, std::function<void(const QList<QUrl>&)> done)
{
    if (m_dialog) {
        fail(tr("A file dialog is already open."));
        return false;
    }
    clearError();
    auto* dialog = new QFileDialog;
    m_dialog = dialog;
    dialog->setWindowTitle(title);
    dialog->setAccessibleName(title);
    dialog->setFileMode(several ? QFileDialog::ExistingFiles : QFileDialog::ExistingFile);
    dialog->setSupportedSchemes({ QStringLiteral("file") });
    dialog->setWindowModality(Qt::ApplicationModal);
    dialog->setDirectory(QDir::homePath());
    connect(dialog, &QDialog::finished, this, [this, dialog, done = std::move(done)](int result) {
        m_dialog.clear();
        emit busyChanged();
        done(result == QDialog::Accepted ? dialog->selectedUrls() : QList<QUrl> {});
        dialog->deleteLater();
    });
    dialog->setAttribute(Qt::WA_NativeWindow);
    if (dialog->windowHandle() && m_window)
        dialog->windowHandle()->setTransientParent(m_window);
    dialog->show();
    emit busyChanged();
    return true;
}
void DesktopFileIntegration::cancelDirectory()
{
    if (m_dialog)
        m_dialog->reject();
}
bool DesktopFileIntegration::openPath(const QString& path)
{
    const auto canonical = canonicalLocalPath(path);
    if (canonical.isEmpty()) {
        fail(tr("This local file or folder is missing, or Kodosi cannot open it."));
        return false;
    }
    if (!m_opener(QUrl::fromLocalFile(canonical))) {
        fail(tr("No application could open this file."));
        return false;
    }
    clearError();
    return true;
}
void DesktopFileIntegration::showInFolder(const QString& path)
{
    const auto file = canonicalLocalPath(path);
    if (file.isEmpty()) {
        fail(tr("This local file or folder is missing, or Kodosi cannot open it."));
        return;
    }
    const auto folder = QFileInfo(file).absolutePath();
#ifdef Q_OS_WIN
    QProcess explorer;
    explorer.setProgram(QStringLiteral("explorer.exe"));
    // Explorer finds the file only when the quotes hold the path alone.
    explorer.setNativeArguments(QStringLiteral("/select,\"%1\"").arg(QDir::toNativeSeparators(file)));
    if (explorer.startDetached())
        clearError();
    else
        openPath(folder);
#else
    const auto openFolder = [this, folder](const QString& reason) {
        qInfo().noquote() << "the file manager did not show the file, so its folder opens:" << reason;
        openPath(folder);
    };
    const auto bus = QDBusConnection::sessionBus();
    if (!bus.isConnected()) {
        openFolder(bus.lastError().message());
        return;
    }
    auto call = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.FileManager1"),
        QStringLiteral("/org/freedesktop/FileManager1"), QStringLiteral("org.freedesktop.FileManager1"),
        QStringLiteral("ShowItems"));
    call << QStringList { QUrl::fromLocalFile(file).toString(QUrl::FullyEncoded) } << QString();
    auto* pending = new QDBusPendingCallWatcher(bus.asyncCall(call), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this, [this, openFolder](QDBusPendingCallWatcher* answer) {
        answer->deleteLater();
        if (answer->isError())
            openFolder(answer->error().message());
        else
            clearError();
    });
#endif
}
void DesktopFileIntegration::saveCopy(const QString& name, const QByteArray& content)
{
    if (m_dialog) {
        fail(tr("A file dialog is already open."));
        return;
    }
    clearError();
    auto* dialog = new QFileDialog;
    m_dialog = dialog;
    dialog->setWindowTitle(tr("Save a copy"));
    dialog->setAccessibleName(dialog->windowTitle());
    dialog->setAcceptMode(QFileDialog::AcceptSave);
    dialog->setFileMode(QFileDialog::AnyFile);
    dialog->setSupportedSchemes({ QStringLiteral("file") });
    dialog->setWindowModality(Qt::ApplicationModal);
    dialog->setDirectory(QStandardPaths::writableLocation(QStandardPaths::DownloadLocation));
    dialog->selectFile(QFileInfo(name).fileName());
    connect(dialog, &QDialog::finished, this, [this, dialog, content](int result) {
        m_dialog.clear();
        emit busyChanged();
        const auto urls = dialog->selectedUrls();
        if (result == QDialog::Accepted && urls.size() == 1 && urls.first().isLocalFile()) {
            QSaveFile file(urls.first().toLocalFile());
            if (!file.open(QIODevice::WriteOnly) || file.write(content) != content.size() || !file.commit())
                fail(tr("Kodosi could not save the copy."));
        }
        dialog->deleteLater();
    });
    dialog->setAttribute(Qt::WA_NativeWindow);
    if (dialog->windowHandle() && m_window)
        dialog->windowHandle()->setTransientParent(m_window);
    dialog->show();
    emit busyChanged();
}
void DesktopFileIntegration::copyText(const QString& text)
{
    QGuiApplication::clipboard()->setText(text);
}

bool DesktopFileIntegration::openWebUrl(const QString& value)
{
    const QUrl url(value, QUrl::StrictMode);
    // Plain HTTP is allowed only for a development sign-in server on this computer.
    const bool local = url.scheme() == QStringLiteral("http")
        && (url.host() == QStringLiteral("127.0.0.1") || url.host() == QStringLiteral("localhost")
            || url.host() == QStringLiteral("::1"));
    if (value.size() > 4096 || !url.isValid() || (url.scheme() != QStringLiteral("https") && !local)
        || url.host().isEmpty() || !url.userInfo().isEmpty()) {
        fail(tr("The sign-in address is invalid."));
        return false;
    }
    if (!m_opener(url)) {
        fail(tr("The browser could not open the sign-in address."));
        return false;
    }
    clearError();
    return true;
}
void DesktopFileIntegration::clearError()
{
    if (!m_error.isEmpty()) {
        m_error.clear();
        emit errorChanged();
    }
}
void DesktopFileIntegration::fail(QString error)
{
    m_error = std::move(error);
    emit errorChanged();
}
}
