#pragma once

#include "platform/DesktopNotificationDriver.hpp"

#include <memory>

namespace kodosi {

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
