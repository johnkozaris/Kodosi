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

constexpr wchar_t group[] = L"kodosi";

winrt::hstring wide(const QString& text)
{
    return { reinterpret_cast<const wchar_t*>(text.utf16()), static_cast<winrt::hstring::size_type>(text.size()) };
}

// Windows limits a toast tag to 64 characters, so the tag is a digest of the key.
winrt::hstring tag(const QString& key)
{
    return wide(QString::fromLatin1(QCryptographicHash::hash(key.toUtf8(), QCryptographicHash::Sha256).toHex().left(32)));
}

XmlDocument content(const DesktopNotification& notification)
{
    XmlDocument xml;
    xml.LoadXml(
        LR"(<toast activationType="protocol"><visual><binding template="ToastGeneric"><text/><text/></binding></visual></toast>)");
    xml.DocumentElement().SetAttribute(L"launch", wide(DeepLinkRouter::url(notification.destination)));
    const auto texts = xml.GetElementsByTagName(L"text");
    texts.Item(0).InnerText(wide(notification.title));
    texts.Item(1).InnerText(wide(notification.body));
    return xml;
}

}

// Toast callbacks arrive on Windows threads and may outlive the driver, so
// they reach it only through this relay while it exists.
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
        notifications::ToastNotificationManager::History().RemoveGroup(group);
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
    try {
        notifications::ToastNotification toast(content(notification));
        toast.Tag(tag(key));
        toast.Group(group);
        toast.Dismissed([relay = m_relay, key](const auto&, const notifications::ToastDismissedEventArgs& args) {
            if (args.Reason() == notifications::ToastDismissalReason::UserCanceled)
                relay->post([key](ToastNotificationDriver& driver) { emit driver.notificationClosed(key); });
        });
        toast.Failed([relay = m_relay, key](const auto&, const auto&) {
            relay->post([key](ToastNotificationDriver& driver) { emit driver.notificationClosed(key); });
        });
        notifications::ToastNotificationManager::CreateToastNotifier().Show(toast);
    } catch (const winrt::hresult_error&) {
        emit notificationClosed(key);
    }
}

void ToastNotificationDriver::withdraw(const QString& key)
{
    try {
        notifications::ToastNotificationManager::History().Remove(tag(key), group);
    } catch (const winrt::hresult_error&) {
    }
}

}
