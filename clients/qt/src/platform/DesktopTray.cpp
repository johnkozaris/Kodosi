#include "platform/DesktopTray.hpp"

#include "presentation/DesktopSettings.hpp"

#include <QIcon>
#include <QMenu>
#include <QSystemTrayIcon>

namespace kodosi {

DesktopTray::DesktopTray(DesktopSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
    if (!QSystemTrayIcon::isSystemTrayAvailable())
        return;
    m_menu = std::make_unique<QMenu>();
    m_menu->addAction(tr("Open Kodosi"), this, &DesktopTray::openRequested);
    m_menu->addSeparator();
    m_menu->addAction(tr("Quit Kodosi"), this, &DesktopTray::quitRequested);
    m_icon = std::make_unique<QSystemTrayIcon>(QIcon(QStringLiteral(":/kodosi/kodosi-tray.svg")));
    m_icon->setToolTip(QStringLiteral("Kodosi"));
    m_icon->setContextMenu(m_menu.get());
    connect(m_icon.get(), &QSystemTrayIcon::activated, this, [this](QSystemTrayIcon::ActivationReason reason) {
        if (reason == QSystemTrayIcon::Trigger || reason == QSystemTrayIcon::DoubleClick)
            emit openRequested();
    });
    connect(&m_settings, &DesktopSettings::settingsChanged, this, &DesktopTray::follow);
    follow();
}

DesktopTray::~DesktopTray() = default;

void DesktopTray::follow()
{
    m_icon->setVisible(m_settings.keepRunning());
}

void DesktopTray::noteHidden()
{
    if (!m_icon || !m_icon->isVisible() || m_settings.closeHintShown())
        return;
    m_settings.setCloseHintShown();
    m_icon->showMessage(tr("Kodosi keeps running"),
        tr("Your terminals keep running. Open or quit Kodosi from the system tray."), m_icon->icon(), 8000);
}

}
