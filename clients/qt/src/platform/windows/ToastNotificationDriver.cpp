#include "platform/windows/ToastNotificationDriver.hpp"

#include <QCryptographicHash>
#include <QMetaObject>

#include <mutex>
#include <utility>

#include <winrt/Windows.Data.Xml.Dom.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.Notifications.h>

namespace kodosi {
namespace {

namespace notifications = winrt::Windows::UI::Notifications;
using winrt::Windows::Data::Xml::Dom::XmlDocument;

winrt::hstring wide(const QString& text)
{
    return { reinterpret_cast<const wchar_t*>(text.utf16()), static_cast<winrt::hstring::size_type>(text.size()) };
}

// Windows limits toast tags and groups to 64 characters.
winrt::hstring shortened(const QString& text)
{
    return wide(QString::fromLatin1(QCryptographicHash::hash(text.toUtf8(), QCryptographicHash::Sha256).toHex().left(32)));
}

winrt::hstring group(const QString& computer)
{
    return shortened(computer.isEmpty() ? QStringLiteral("this computer") : computer);
}

XmlDocument content(const DesktopNotification& notification)
{
    XmlDocument xml;
    xml.LoadXml(
        LR"(<toast activationType="protocol"><visual><binding template="ToastGeneric"><text/><text/></binding></visual></toast>)");
    xml.DocumentElement().SetAttribute(L"launch",
        wide(DeepLinkRouter::url({ .sessionId = notification.sessionId, .notification = true })));
    const auto texts = xml.GetElementsByTagName(L"text");
    texts.Item(0).InnerText(wide(notification.title));
    texts.Item(1).InnerText(wide(notification.body));
    if (notification.cue != NotificationCue::Sound) {
        const auto audio = xml.CreateElement(L"audio");
        audio.SetAttribute(L"silent", L"true");
        xml.DocumentElement().AppendChild(audio);
    }
    return xml;
}

}

// Toast callbacks arrive on Windows threads and may outlive the driver.
struct ToastNotificationDriver::Relay {
    std::mutex mutex;
    ToastNotificationDriver* driver = nullptr;

    template <typename Action>
    void post(Action action)
    {
        const std::lock_guard lock(mutex);
        if (driver)
            QMetaObject::invokeMethod(
                driver, [target = driver, action = std::move(action)] { action(*target); }, Qt::QueuedConnection);
    }
};

ToastNotificationDriver::ToastNotificationDriver(QObject* parent)
    : DesktopNotificationDriver(parent)
    , m_relay(std::make_shared<Relay>())
{
    m_relay->driver = this;
}

ToastNotificationDriver::~ToastNotificationDriver()
{
    {
        const std::lock_guard lock(m_relay->mutex);
        m_relay->driver = nullptr;
    }
    try {
        notifications::ToastNotificationManager::History().Clear();
    } catch (const winrt::hresult_error&) {
    }
}

void ToastNotificationDriver::post(const DesktopNotification& notification)
{
    if (!notification.valid()) {
        emit notificationClosed(notification.key);
        return;
    }
    const auto key = notification.key;
    if (const auto computer = m_computers.constFind(key);
        computer != m_computers.cend() && computer.value() != notification.computer)
        withdraw(key);
    try {
        notifications::ToastNotification toast(content(notification));
        toast.Tag(shortened(key));
        toast.Group(group(notification.computer));
        toast.SuppressPopup(notification.cue == NotificationCue::List);
        toast.Dismissed([relay = m_relay, key](const auto&, const notifications::ToastDismissedEventArgs& args) {
            if (args.Reason() == notifications::ToastDismissalReason::UserCanceled)
                relay->post([key](ToastNotificationDriver& driver) {
                    driver.m_computers.remove(key);
                    emit driver.notificationClosed(key);
                });
        });
        toast.Failed([relay = m_relay, key](const auto&, const auto&) {
            relay->post([key](ToastNotificationDriver& driver) {
                driver.m_computers.remove(key);
                emit driver.notificationClosed(key);
            });
        });
        notifications::ToastNotificationManager::CreateToastNotifier().Show(toast);
        m_computers.insert(key, notification.computer);
    } catch (const winrt::hresult_error&) {
        emit notificationClosed(key);
    }
}

void ToastNotificationDriver::withdraw(const QString& key)
{
    const auto computer = m_computers.find(key);
    if (computer == m_computers.end())
        return;
    try {
        notifications::ToastNotificationManager::History().Remove(shortened(key), group(computer.value()));
    } catch (const winrt::hresult_error&) {
    }
    m_computers.erase(computer);
}

}
