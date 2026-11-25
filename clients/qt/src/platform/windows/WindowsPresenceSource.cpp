#include "platform/windows/WindowsPresenceSource.hpp"

#include <QMetaObject>

#include <utility>

#include <windows.h>

#include <wtsapi32.h>

namespace kodosi {
namespace {

using namespace std::chrono_literals;

constexpr wchar_t windowClass[] = L"KodosiPresence";
// GUID_SESSION_DISPLAY_STATUS (session display on, off, or dimmed).
constexpr GUID sessionDisplayStatus { 0x2b84c20e, 0xad23, 0x4ddf, { 0x93, 0xdb, 0x05, 0xff, 0xbd, 0x7e, 0xfc, 0xa5 } };
// Windows exposes last-input time via polling rather than events. Poll every 10s while idle so
// new input is detected well within the server's 30s push-notification delay.
constexpr std::chrono::milliseconds readWhileQuiet = 10s;

LRESULT CALLBACK windowProcedure(HWND window, UINT message, WPARAM parameter, LPARAM detail)
{
    if (auto* source = reinterpret_cast<WindowsPresenceSource*>(GetWindowLongPtrW(window, GWLP_USERDATA)))
        source->receive(message, parameter, detail);
    if (message == WM_POWERBROADCAST)
        return TRUE;
    return DefWindowProcW(window, message, parameter, detail);
}

}

WindowsPresenceSource::WindowsPresenceSource(QObject* parent)
    : PresenceSource(parent)
{
    WTSINFOEXW* session = nullptr;
    DWORD bytes = 0;
    if (WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTSSessionInfoEx,
            reinterpret_cast<LPWSTR*>(&session), &bytes)
        && session && bytes >= sizeof(WTSINFOEXW) && session->Level == 1) {
        m_locked = session->Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK;
        m_disconnected = session->Data.WTSInfoExLevel1.SessionState == WTSDisconnected;
    }
    if (session)
        WTSFreeMemory(session);

    const auto instance = GetModuleHandleW(nullptr);
    WNDCLASSEXW type {};
    type.cbSize = sizeof(type);
    type.lpfnWndProc = windowProcedure;
    type.hInstance = instance;
    type.lpszClassName = windowClass;
    RegisterClassExW(&type);
    // Windows only sends suspend/resume broadcasts to top-level windows, not message-only ones.
    auto* window = CreateWindowExW(0, windowClass, L"", WS_POPUP, 0, 0, 0, 0, nullptr, nullptr, instance, nullptr);
    if (!window)
        return;
    m_window = window;
    SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
    WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
    m_displayNotices = RegisterPowerSettingNotification(window, &sessionDisplayStatus, DEVICE_NOTIFY_WINDOW_HANDLE);
}

WindowsPresenceSource::~WindowsPresenceSource()
{
    if (!m_window)
        return;
    if (m_displayNotices)
        UnregisterPowerSettingNotification(static_cast<HPOWERNOTIFY>(m_displayNotices));
    auto* window = static_cast<HWND>(m_window);
    WTSUnRegisterSessionNotification(window);
    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    DestroyWindow(window);
}

PresenceSource::Input WindowsPresenceSource::input()
{
    LASTINPUTINFO last {};
    last.cbSize = sizeof(last);
    if (!GetLastInputInfo(&last))
        return { .quiet = false, .readAgainIn = readWhileQuiet };
    const std::chrono::milliseconds age(static_cast<DWORD>(GetTickCount() - last.dwTime));
    if (age >= m_awayTime)
        return { .quiet = true, .readAgainIn = readWhileQuiet };
    return { .quiet = false, .readAgainIn = m_awayTime - age };
}

void WindowsPresenceSource::setAwayTime(const std::chrono::milliseconds time)
{
    m_awayTime = time;
}

void WindowsPresenceSource::receive(const unsigned int message, const std::uintptr_t parameter, const std::intptr_t detail)
{
    if (message == WM_WTSSESSION_CHANGE) {
        qCInfo(kodosiPresence).noquote() << "Session notice" << parameter;
        if (parameter == WTS_SESSION_LOCK || parameter == WTS_SESSION_UNLOCK)
            set(m_locked, parameter == WTS_SESSION_LOCK);
        else if (parameter == WTS_CONSOLE_DISCONNECT || parameter == WTS_REMOTE_DISCONNECT)
            set(m_disconnected, true);
        else if (parameter == WTS_CONSOLE_CONNECT || parameter == WTS_REMOTE_CONNECT)
            set(m_disconnected, false);
    } else if (message == WM_POWERBROADCAST && parameter == PBT_APMSUSPEND) {
        set(m_sleeping, true);
    } else if (message == WM_POWERBROADCAST && (parameter == PBT_APMRESUMESUSPEND || parameter == PBT_APMRESUMEAUTOMATIC)) {
        set(m_sleeping, false);
    } else if (message == WM_POWERBROADCAST && parameter == PBT_POWERSETTINGCHANGE) {
        const auto* setting = reinterpret_cast<const POWERBROADCAST_SETTING*>(detail);
        if (setting && IsEqualGUID(setting->PowerSetting, sessionDisplayStatus) && setting->DataLength == sizeof(DWORD))
            set(m_displayOff, *reinterpret_cast<const DWORD*>(setting->Data) == 0);
    }
}

void WindowsPresenceSource::set(bool& fact, const bool value)
{
    if (std::exchange(fact, value) != value)
        QMetaObject::invokeMethod(this, &PresenceSource::changed, Qt::QueuedConnection);
}

}
