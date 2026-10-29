#include "canvas/CanvasWeb.hpp"
#include "presentation/CanvasModel.hpp"

#include <QBuffer>
#include <QtWebEngineCore/QWebEngineUrlRequestInfo>
#include <QtWebEngineCore/QWebEngineUrlRequestInterceptor>
#include <QtWebEngineCore/QWebEngineUrlRequestJob>
#include <QtWebEngineCore/QWebEngineUrlScheme>
#include <QtWebEngineCore/QWebEngineUrlSchemeHandler>
#include <QtWebEngineQuick/QQuickWebEngineDownloadRequest>

#ifdef Q_OS_LINUX
#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <sched.h>
#include <string>
#include <sys/wait.h>
#include <unistd.h>
#endif

namespace kodosi {
namespace {
#ifdef Q_OS_LINUX
// The check of Chromium's namespace sandbox: a child in a new user namespace maps its
// user and makes one more user namespace.
bool sandboxed()
{
    if (geteuid() == 0)
        return false;
    const auto users = std::to_string(getuid()) + ' ' + std::to_string(getuid()) + " 1";
    const auto groups = std::to_string(getgid()) + ' ' + std::to_string(getgid()) + " 1";
    const pid_t child = fork();
    if (child < 0)
        return false;
    if (child == 0) {
        const auto put = [](const char* path, const char* text) {
            const auto size = static_cast<ssize_t>(std::strlen(text));
            const int file = open(path, O_WRONLY | O_CLOEXEC);
            const bool written = file >= 0 && write(file, text, static_cast<size_t>(size)) == size;
            if (file >= 0)
                close(file);
            return written;
        };
        const bool made = unshare(CLONE_NEWUSER) == 0 && put("/proc/self/setgroups", "deny")
            && put("/proc/self/uid_map", users.c_str()) && put("/proc/self/gid_map", groups.c_str())
            && unshare(CLONE_NEWUSER) == 0;
        _exit(made ? 0 : 1);
    }
    int status = 0;
    while (waitpid(child, &status, 0) < 0) {
        if (errno != EINTR)
            return false;
    }
    return WIFEXITED(status) && WEXITSTATUS(status) == 0;
}
#else
bool sandboxed()
{
    return true;
}
#endif

const auto scheme = QByteArrayLiteral("kodosi-canvas");
const auto policy = QByteArrayLiteral(
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; "
    "font-src data:; media-src data: blob:; form-action 'none'; base-uri 'none'");

class PageHandler final : public QWebEngineUrlSchemeHandler {
public:
    PageHandler(CanvasModel& canvas, QObject* parent)
        : QWebEngineUrlSchemeHandler(parent)
        , m_canvas(canvas)
    {
    }

    void requestStarted(QWebEngineUrlRequestJob* job) override
    {
        const auto page = job->requestMethod() == QByteArrayLiteral("GET") ? m_canvas.published(job->requestUrl())
                                                                           : std::nullopt;
        if (!page) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }
        QMultiMap<QByteArray, QByteArray> headers { { QByteArrayLiteral("Cache-Control"), QByteArrayLiteral("no-store") } };
        if (page->first.startsWith("text/html"))
            headers.insert(QByteArrayLiteral("Content-Security-Policy"), policy);
        job->setAdditionalResponseHeaders(headers);
        auto* content = new QBuffer(job);
        content->setData(page->second);
        job->reply(page->first, content);
    }

private:
    CanvasModel& m_canvas;
};

class RequestGuard final : public QWebEngineUrlRequestInterceptor {
public:
    using QWebEngineUrlRequestInterceptor::QWebEngineUrlRequestInterceptor;

    void interceptRequest(QWebEngineUrlRequestInfo& info) override
    {
        const auto place = info.requestUrl().scheme().toLatin1();
        info.block(place != scheme && place != "data" && place != "blob" && place != "about" && place != "chrome"
            && place != "chrome-extension");
    }
};
}

CanvasWeb::CanvasWeb(CanvasModel& canvas, QObject* parent)
    : QObject(parent)
    , m_canvas(canvas)
    , m_available(sandboxed())
{
}

void CanvasWeb::registerScheme()
{
    QWebEngineUrlScheme page(scheme);
    page.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    page.setFlags(QWebEngineUrlScheme::SecureScheme);
    QWebEngineUrlScheme::registerScheme(page);
}

QQuickWebEngineProfile* CanvasWeb::profile()
{
    if (m_profile)
        return m_profile;
    m_profile = QQuickWebEngineProfile::defaultProfile();
    m_profile->installUrlSchemeHandler(scheme, new PageHandler(m_canvas, m_profile));
    m_profile->setUrlRequestInterceptor(new RequestGuard(m_profile));
    connect(m_profile, &QQuickWebEngineProfile::downloadRequested, m_profile,
        [](QQuickWebEngineDownloadRequest* download) { download->cancel(); });
    return m_profile;
}
}
