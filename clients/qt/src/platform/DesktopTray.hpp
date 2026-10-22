#pragma once

#include <QObject>
#include <memory>

class QMenu;
class QSystemTrayIcon;

namespace kodosi {

class DesktopSettings;

class DesktopTray final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available CONSTANT)
public:
    explicit DesktopTray(DesktopSettings& settings, QObject* parent = nullptr);
    ~DesktopTray() override;
    DesktopTray(const DesktopTray&) = delete;
    DesktopTray& operator=(const DesktopTray&) = delete;

    [[nodiscard]] bool available() const noexcept { return m_icon != nullptr; }
    Q_INVOKABLE void noteHidden();

signals:
    void openRequested();
    void quitRequested();

private:
    DesktopSettings& m_settings;
    std::unique_ptr<QMenu> m_menu;
    std::unique_ptr<QSystemTrayIcon> m_icon;

    void follow();
};

}
