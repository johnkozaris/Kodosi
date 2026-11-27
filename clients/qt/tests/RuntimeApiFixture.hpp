#pragma once
#include "runtime/RuntimeBridge.hpp"
#include <QByteArray>
#include <QStringList>
#include <kodosi_runtime.h>
#include <optional>

namespace runtime_fixture {
inline kodosi_callbacks_t callbacks {};
inline void* userdata = nullptr;
inline QByteArray lastCommand;
inline bool allowStart = false;
inline std::optional<kodosi_start_failure_t> startFailure;
inline int hostStops = 0;
// "watch <pipe>" and "unwatch <pipe>", in the order that the app called them.
inline QStringList videoCalls;
inline void sendEvent(const QByteArray& event)
{
    if (callbacks.on_event)
        callbacks.on_event(reinterpret_cast<const uint8_t*>(event.constData()),
            static_cast<uintptr_t>(event.size()), userdata);
}
inline void reset()
{
    callbacks = {};
    userdata = nullptr;
    lastCommand.clear();
    allowStart = false;
    startFailure.reset();
    hostStops = 0;
    videoCalls.clear();
}
}
