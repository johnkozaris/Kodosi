#include "platform/DesktopFileIntegration.hpp"
#include <QDesktopServices>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QWindow>

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
    if (path.isEmpty() || path.size() > 4096 || path.contains(QChar::Null) || !QDir::isAbsolutePath(path)
        || !QUrl(path).scheme().isEmpty())
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
    dialog->setWindowTitle(tr("Choose a project folder"));
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
        } else
            emit directoryPickCancelled(purpose);
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
        fail(tr("This local file or folder is missing or cannot be opened."));
        return false;
    }
    if (!m_opener(QUrl::fromLocalFile(canonical))) {
        fail(tr("No application could open this file."));
        return false;
    }
    clearError();
    return true;
}
bool DesktopFileIntegration::openWebUrl(const QString& value)
{
    const QUrl url(value, QUrl::StrictMode);
    if (value.size() > 4096 || !url.isValid() || url.scheme() != QStringLiteral("https")
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
