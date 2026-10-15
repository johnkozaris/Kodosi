#include "platform/windows/WindowFrame.hpp"

#include <QCoreApplication>
#include <QTimer>
#include <QWindow>
#include <qpa/qplatformwindow.h>
#include <qpa/qplatformwindow_p.h>

#include <windows.h>

#include <dwmapi.h>
#include <shellapi.h>

#include <array>

namespace kodosi {
namespace {

COLORREF colorRef(const QColor& color)
{
    return RGB(color.red(), color.green(), color.blue());
}

// Windows keeps an auto-hide taskbar hidden behind a window that covers its
// monitor, so a maximized window leaves the taskbar's edge to the pointer.
void leaveAutoHideTaskbars(HWND hwnd, RECT& client)
{
    APPBARDATA state {};
    state.cbSize = sizeof(state);
    if ((SHAppBarMessage(ABM_GETSTATE, &state) & ABS_AUTOHIDE) == 0)
        return;
    MONITORINFO monitor {};
    monitor.cbSize = sizeof(monitor);
    if (!GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &monitor))
        return;
    constexpr LONG reveal = 2;
    for (const UINT edge : std::array<UINT, 4> { ABE_LEFT, ABE_TOP, ABE_RIGHT, ABE_BOTTOM }) {
        APPBARDATA bar {};
        bar.cbSize = sizeof(bar);
        bar.uEdge = edge;
        bar.rc = monitor.rcMonitor;
        if (SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &bar) == 0)
            continue;
        if (edge == ABE_LEFT)
            client.left += reveal;
        else if (edge == ABE_TOP)
            client.top += reveal;
        else if (edge == ABE_RIGHT)
            client.right -= reveal;
        else
            client.bottom -= reveal;
    }
}

}

WindowFrame::WindowFrame(QWindow& window)
    : QObject(&window)
    , m_window(window)
    , m_handle(reinterpret_cast<void*>(window.winId()))
{
    const auto hwnd = static_cast<HWND>(m_handle);
    const auto style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX);
    const COLORREF border = DWMWA_COLOR_NONE;
    DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border, sizeof(border));
    QCoreApplication::instance()->installNativeEventFilter(this);
    SetWindowPos(hwnd, nullptr, 0, 0, 0, 0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE);
    alignClient();
    connect(&window, &QWindow::screenChanged, this, [this] { QTimer::singleShot(0, this, &WindowFrame::alignClient); });
}

WindowFrame::~WindowFrame()
{
    if (auto* application = QCoreApplication::instance())
        application->removeNativeEventFilter(this);
}

void WindowFrame::setColor(const QColor& color)
{
    m_color = color;
    const auto hwnd = static_cast<HWND>(m_handle);
    const auto caption = colorRef(color);
    DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption, sizeof(caption));
    RedrawWindow(hwnd, nullptr, nullptr, RDW_FRAME | RDW_INVALIDATE | RDW_UPDATENOW);
}

bool WindowFrame::nativeEventFilter(const QByteArray& eventType, void* message, qintptr* result)
{
    if (eventType != "windows_generic_MSG")
        return false;
    const auto* msg = static_cast<const MSG*>(message);
    if (msg->hwnd != static_cast<HWND>(m_handle))
        return false;
    switch (msg->message) {
    case WM_NCCALCSIZE:
        if (!msg->wParam || !fitClient(reinterpret_cast<NCCALCSIZE_PARAMS*>(msg->lParam)))
            return false;
        *result = 0;
        return true;
    case WM_NCPAINT:
        paint();
        *result = 0;
        return true;
    case WM_NCACTIVATE:
        // The default handler would repaint the frame in the system colours.
        *result = DefWindowProcW(msg->hwnd, WM_NCACTIVATE, msg->wParam, -1);
        return true;
    default:
        return false;
    }
}

// Qt 6.12 leaves a thin native strip between its expanded client area and the
// invisible resize border, which Windows composes inconsistently. The client
// covers the whole window instead, so no native frame shows, and Qt's resize
// border sits just inside the window edge.
bool WindowFrame::fitClient(void* parameters) const
{
    const auto hwnd = static_cast<HWND>(m_handle);
    auto& client = static_cast<NCCALCSIZE_PARAMS*>(parameters)->rgrc[0];
    if (IsIconic(hwnd))
        return false;
    if (IsZoomed(hwnd)) {
        // A maximized window hangs over the screen by its resize border.
        const auto dpi = GetDpiForWindow(hwnd);
        const auto border = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        InflateRect(&client, -border, -border);
        leaveAutoHideTaskbars(hwnd, client);
    }
    return true;
}

// Qt 6.12 derives the frame of an expanded client area from the window style,
// not from the client area the frame really leaves. Its custom margins make up
// the difference, so the scene fills the native client area and the saved
// geometry restores to the same place.
void WindowFrame::alignClient()
{
    auto* platform = m_window.handle();
    auto* native = m_window.nativeInterface<QNativeInterface::Private::QWindowsWindow>();
    if (!platform || !native || m_window.windowState() != Qt::WindowNoState)
        return;
    const auto hwnd = static_cast<HWND>(m_handle);
    RECT client {};
    POINT origin {};
    if (!GetClientRect(hwnd, &client) || !ClientToScreen(hwnd, &origin))
        return;
    const QRect actual(origin.x, origin.y, client.right, client.bottom);
    const auto assumed = platform->geometry();
    const QMargins error(actual.left() - assumed.left(), actual.top() - assumed.top(),
        assumed.right() - actual.right(), assumed.bottom() - actual.bottom());
    if (!error.isNull())
        native->setCustomMargins(native->customMargins() + error);
}

void WindowFrame::paint() const
{
    const auto hwnd = static_cast<HWND>(m_handle);
    RECT window {};
    RECT client {};
    GetWindowRect(hwnd, &window);
    GetClientRect(hwnd, &client);
    MapWindowPoints(hwnd, HWND_DESKTOP, reinterpret_cast<POINT*>(&client), 2);
    OffsetRect(&client, -window.left, -window.top);
    const RECT frame { 0, 0, window.right - window.left, window.bottom - window.top };
    const auto context = GetWindowDC(hwnd);
    if (!context)
        return;
    ExcludeClipRect(context, client.left, client.top, client.right, client.bottom);
    const auto brush = CreateSolidBrush(colorRef(m_color));
    FillRect(context, &frame, brush);
    DeleteObject(brush);
    ReleaseDC(hwnd, context);
}

}
