#pragma once
#include "runtime/RuntimeBridge.hpp"
#include <QByteArray>
#include <kodosi_runtime.h>
#include <optional>

namespace runtime_fixture {
inline kodosi_callbacks_t callbacks {};
inline void* userdata = nullptr;
inline QByteArray lastCommand;
inline bool allowStart = false;
inline uint32_t protocolVersion = kodosi::RuntimeBridge::protocolVersion;
inline int commandResult = KODOSI_FFI_OK;
inline std::optional<kodosi_start_failure_t> startFailure;
inline int hostStops = 0;
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
    protocolVersion = kodosi::RuntimeBridge::protocolVersion;
    commandResult = KODOSI_FFI_OK;
    startFailure.reset();
    hostStops = 0;
}
}
