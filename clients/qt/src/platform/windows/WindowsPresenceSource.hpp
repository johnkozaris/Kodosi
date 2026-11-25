#pragma once

#include "platform/Presence.hpp"

#include <cstdint>

namespace kodosi {

class WindowsPresenceSource final : public PresenceSource {
    Q_OBJECT

public:
    explicit WindowsPresenceSource(QObject* parent = nullptr);
    ~WindowsPresenceSource() override;

    [[nodiscard]] bool reportsInput() const override { return true; }
    [[nodiscard]] Input input() override;
    [[nodiscard]] bool away() const override { return m_locked || m_disconnected || m_displayOff || m_sleeping; }
    void setAwayTime(std::chrono::milliseconds time) override;

    // Entry point for the hidden top-level window's WndProc.
    void receive(unsigned int message, std::uintptr_t parameter, std::intptr_t detail);

private:
    void* m_window = nullptr;
    void* m_displayNotices = nullptr;
    std::chrono::milliseconds m_awayTime {};
    bool m_locked = false;
    bool m_disconnected = false;
    bool m_displayOff = false;
    bool m_sleeping = false;

    void set(bool& fact, bool value);
};

}
