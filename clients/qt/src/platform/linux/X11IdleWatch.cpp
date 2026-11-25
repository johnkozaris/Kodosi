#include "platform/linux/IdleWatch.hpp"

#include <QAbstractNativeEventFilter>
#include <QByteArray>
#include <QCoreApplication>
#include <QGuiApplication>

#include <cstdint>
#include <cstring>
#include <utility>

#include <X11/Xlib.h>
#include <X11/extensions/sync.h>

namespace kodosi {
namespace {

class X11IdleWatch final : public IdleWatch, public QAbstractNativeEventFilter {
public:
    X11IdleWatch(Display* display, const int alarmEvent, const XSyncCounter counter, Quiet quiet)
        : m_display(display)
        , m_alarmEvent(alarmEvent)
        , m_counter(counter)
        , m_quiet(std::move(quiet))
    {
        QCoreApplication::instance()->installNativeEventFilter(this);
    }

    ~X11IdleWatch() override
    {
        QCoreApplication::instance()->removeNativeEventFilter(this);
        if (m_alarm != None) {
            XSyncDestroyAlarm(m_display, m_alarm);
            XFlush(m_display);
        }
    }

    void watch(const std::chrono::milliseconds quietTime) override
    {
        m_quietTime = quietTime;
        arm();
    }

    bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr*) override
    {
        if (eventType == "xcb_generic_event_t" && m_quietTime.count() > 0
            && (*static_cast<const std::uint8_t*>(message) & 0x7f) == m_alarmEvent)
            arm();
        return false;
    }

private:
    Display* m_display;
    int m_alarmEvent;
    XSyncCounter m_counter;
    Quiet m_quiet;
    XSyncAlarm m_alarm = None;
    std::chrono::milliseconds m_quietTime {};

    // IDLETIME tracks milliseconds since the last input. Arm a positive alarm to detect reaching
    // quietTime, and once idle, arm a negative alarm to fire when new input resets the counter.
    void arm()
    {
        XSyncValue idle;
        if (!XSyncQueryCounter(m_display, m_counter, &idle))
            return;
        const auto milliseconds = (static_cast<std::int64_t>(XSyncValueHigh32(idle)) << 32) | XSyncValueLow32(idle);
        const bool quiet = milliseconds >= m_quietTime.count();
        const auto wait = quiet ? milliseconds - 1 : static_cast<std::int64_t>(m_quietTime.count());

        XSyncAlarmAttributes attributes {};
        attributes.trigger.counter = m_counter;
        attributes.trigger.value_type = XSyncAbsolute;
        XSyncIntsToValue(&attributes.trigger.wait_value, static_cast<unsigned int>(wait & 0xffffffff),
            static_cast<int>(wait >> 32));
        attributes.trigger.test_type = quiet ? XSyncNegativeComparison : XSyncPositiveComparison;
        XSyncIntToValue(&attributes.delta, 0);
        attributes.events = True;
        constexpr unsigned long fields
            = XSyncCACounter | XSyncCAValueType | XSyncCAValue | XSyncCATestType | XSyncCADelta | XSyncCAEvents;
        if (m_alarm == None)
            m_alarm = XSyncCreateAlarm(m_display, fields, &attributes);
        else
            XSyncChangeAlarm(m_display, m_alarm, fields, &attributes);
        XFlush(m_display);
        m_quiet(quiet);
    }
};

}

std::unique_ptr<IdleWatch> x11IdleWatch(IdleWatch::Quiet quiet)
{
    // Under XWayland, the X server only observes input directed at X11 windows.
    if (QGuiApplication::platformName() != QStringLiteral("xcb") || qEnvironmentVariableIsSet("WAYLAND_DISPLAY"))
        return nullptr;
    auto* x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>();
    auto* display = x11 ? x11->display() : nullptr;
    int eventBase = 0;
    int errorBase = 0;
    int major = 0;
    int minor = 0;
    if (!display || !XSyncQueryExtension(display, &eventBase, &errorBase)
        || !XSyncInitialize(display, &major, &minor))
        return nullptr;
    int count = 0;
    auto* counters = XSyncListSystemCounters(display, &count);
    XSyncCounter idle = None;
    for (int index = 0; index < count; ++index) {
        if (std::strcmp(counters[index].name, "IDLETIME") == 0)
            idle = counters[index].counter;
    }
    if (counters)
        XSyncFreeSystemCounterList(counters);
    if (idle == None)
        return nullptr;
    return std::make_unique<X11IdleWatch>(display, eventBase + XSyncAlarmNotify, idle, std::move(quiet));
}

}
