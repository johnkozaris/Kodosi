#include "controls/SignClock.hpp"

#include <QCoreApplication>
#include <QDateTime>
#include <QGuiApplication>
#include <QPointer>
#include <QQuickWindow>
#include <QTimer>
#include <rhi/qrhi.h>

#include <algorithm>
#include <atomic>
#include <memory>
#include <utility>
#include <vector>

namespace kodosi {
namespace {

enum class Drawing : int { Unknown, Processor, GraphicsProcessor };

std::atomic<Drawing> drawing { Drawing::Unknown };

bool drawsByProcessor(const QRhi* rhi)
{
    if (rhi == nullptr)
        return false;
    const auto info = rhi->driverInfo();
    if (info.deviceType == QRhiDriverInfo::CpuDevice)
        return true;
    // OpenGL names no type of device, but Mesa's drivers that draw by the processor name themselves.
    return rhi->backend() == QRhi::OpenGLES2
        && (info.deviceName.contains("llvmpipe") || info.deviceName.contains("softpipe"));
}

// A window's QRhi belongs to its render thread, so the window is asked there, after a frame.
void learnDrawing(QQuickWindow* window)
{
    auto connection = std::make_shared<QMetaObject::Connection>();
    *connection = QObject::connect(
        window, &QQuickWindow::afterFrameEnd, window,
        [window, connection] {
            drawing.store(drawsByProcessor(window->rhi()) ? Drawing::Processor : Drawing::GraphicsProcessor);
            QObject::disconnect(*connection);
        },
        Qt::DirectConnection);
}

class Clock final : public QObject {
public:
    Clock()
        : QObject(QCoreApplication::instance())
    {
        m_timer.setSingleShot(true);
        m_timer.setTimerType(Qt::PreciseTimer);
        connect(&m_timer, &QTimer::timeout, this, [this] { tick(); });
    }

    void join(const QObject* sign, std::function<void(std::int64_t)> onStep)
    {
        const auto found = find(sign);
        if (found != m_signs.end())
            found->second = std::move(onStep);
        else
            m_signs.emplace_back(sign, std::move(onStep));
        if (!m_timer.isActive())
            schedule();
    }

    void leave(const QObject* sign)
    {
        const auto found = find(sign);
        if (found != m_signs.end())
            m_signs.erase(found);
        if (m_signs.empty())
            m_timer.stop();
    }

    [[nodiscard]] bool running() const { return m_timer.isActive(); }
    [[nodiscard]] std::int64_t now() const { return m_now; }

private:
    using Signs = std::vector<std::pair<const QObject*, std::function<void(std::int64_t)>>>;

    Signs::iterator find(const QObject* sign)
    {
        return std::find_if(m_signs.begin(), m_signs.end(), [sign](const auto& entry) { return entry.first == sign; });
    }

    void tick()
    {
        m_now = QDateTime::currentMSecsSinceEpoch();
        std::vector<const QObject*> stepping;
        stepping.reserve(m_signs.size());
        for (const auto& entry : m_signs)
            stepping.push_back(entry.first);
        for (const auto* sign : stepping) {
            const auto found = find(sign);
            if (found != m_signs.end()) {
                const auto onStep = found->second;
                onStep(m_now);
            }
        }
        schedule();
    }

    void schedule()
    {
        if (m_signs.empty())
            return;
        if (drawing.load() == Drawing::Unknown) {
            for (auto* window : QGuiApplication::topLevelWindows()) {
                auto* quick = qobject_cast<QQuickWindow*>(window);
                if (quick != nullptr && !m_asked.contains(quick)) {
                    m_asked.push_back(quick);
                    learnDrawing(quick);
                }
            }
        }
        const std::int64_t interval
            = (drawing.load() == Drawing::Processor ? SignClock::processorStep : SignClock::step).count();
        const std::int64_t now = QDateTime::currentMSecsSinceEpoch();
        m_timer.start(std::chrono::milliseconds((now / interval + 1) * interval - now));
    }

    QTimer m_timer;
    Signs m_signs;
    QList<QPointer<QQuickWindow>> m_asked;
    std::int64_t m_now = QDateTime::currentMSecsSinceEpoch();
};

QPointer<Clock> shared;

}

void SignClock::join(const QObject* sign, std::function<void(std::int64_t now)> onStep)
{
    if (QCoreApplication::instance() == nullptr)
        return;
    if (shared.isNull())
        shared = new Clock;
    shared->join(sign, std::move(onStep));
}

void SignClock::leave(const QObject* sign)
{
    if (!shared.isNull())
        shared->leave(sign);
}

bool SignClock::running()
{
    return !shared.isNull() && shared->running();
}

std::int64_t SignClock::now()
{
    return shared.isNull() ? QDateTime::currentMSecsSinceEpoch() : shared->now();
}

SignPhase::SignPhase(QObject* parent)
    : QObject(parent)
{
}

SignPhase::~SignPhase()
{
    if (m_running)
        SignClock::leave(this);
}

void SignPhase::setRunning(const bool running)
{
    if (running == m_running)
        return;
    m_running = running;
    if (running) {
        m_start = QDateTime::currentMSecsSinceEpoch();
        follow(m_start);
        SignClock::join(this, [this](const std::int64_t now) { follow(now); });
    } else {
        SignClock::leave(this);
    }
    emit runningChanged();
}

void SignPhase::setPeriod(const int period)
{
    if (period <= 0 || period == m_period)
        return;
    m_period = period;
    emit periodChanged();
    if (m_running)
        follow(SignClock::now());
}

void SignPhase::follow(const std::int64_t now)
{
    const auto value = static_cast<double>(std::max<std::int64_t>(0, now - m_start) % m_period) / m_period;
    if (value == m_value)
        return;
    m_value = value;
    emit valueChanged();
}

}
