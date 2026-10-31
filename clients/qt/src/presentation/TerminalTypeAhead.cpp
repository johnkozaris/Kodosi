#include "presentation/TerminalTypeAhead.hpp"

#include <QCoreApplication>
#include <QKeyEvent>

#include <chrono>

namespace kodosi {
namespace {

constexpr std::size_t maximumKeys = 512;

}

TerminalTypeAhead::TerminalTypeAhead(QObject* parent)
    : QObject(parent)
{
    m_expiry.setSingleShot(true);
    m_expiry.setInterval(std::chrono::seconds(2));
    connect(&m_expiry, &QTimer::timeout, this, &TerminalTypeAhead::drop);
}

void TerminalTypeAhead::hold()
{
    m_expiry.start();
    if (!m_holding) {
        m_holding = true;
        QCoreApplication::instance()->installEventFilter(this);
    }
}

void TerminalTypeAhead::expect(const QString& sessionId)
{
    if (m_holding)
        m_terminal = sessionId;
}

void TerminalTypeAhead::deliver(const QString& sessionId)
{
    if (!m_holding || sessionId != m_terminal)
        return;
    auto keys = std::make_shared<std::vector<std::unique_ptr<QEvent>>>(std::move(m_keys));
    const auto window = m_window;
    drop();
    QTimer::singleShot(0, this, [keys, window] {
        for (const auto& key : *keys) {
            if (window)
                QCoreApplication::sendEvent(window, key.get());
        }
    });
}

void TerminalTypeAhead::drop()
{
    m_expiry.stop();
    if (m_holding)
        QCoreApplication::instance()->removeEventFilter(this);
    m_holding = false;
    m_window = nullptr;
    m_terminal.clear();
    m_pressed.clear();
    m_keys.clear();
}

bool TerminalTypeAhead::eventFilter(QObject* target, QEvent* event)
{
    if (!target->isWindowType())
        return false;
    const auto type = event->type();
    if (type != QEvent::KeyPress && type != QEvent::KeyRelease)
        return false;
    const auto key = static_cast<QKeyEvent*>(event)->key();
    if (type == QEvent::KeyRelease && !m_pressed.remove(key))
        return false;
    if (type == QEvent::KeyPress)
        m_pressed.insert(key);
    if (m_keys.size() < maximumKeys) {
        m_window = static_cast<QWindow*>(target);
        m_keys.emplace_back(event->clone());
    }
    return true;
}

}
