#pragma once

#include "platform/DesktopNotificationDriver.hpp"

#include <memory>

namespace kodosi {

/// Shows notifications as Windows toasts. Clicking a toast while the app runs
/// reports its default action; the app removes its toasts when it closes,
/// because the terminals they name close with it.
class ToastNotificationDriver final : public DesktopNotificationDriver {
    Q_OBJECT

public:
    explicit ToastNotificationDriver(QObject* parent = nullptr);
    ~ToastNotificationDriver() override;

    void post(const DesktopNotification& notification) override;
    void withdraw(const QString& key) override;

private:
    struct Relay;
    std::shared_ptr<Relay> m_relay;
};

}
