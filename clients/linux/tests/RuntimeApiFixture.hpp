#pragma once
#include <QByteArray>
#include <kodosi_runtime.h>

namespace runtime_fixture {
inline kodosi_callbacks_t callbacks {};
inline void* userdata = nullptr;
inline QByteArray lastCommand;
inline bool allowStart = false;
inline uint32_t protocolVersion = 54;
inline int commandResult = KODOSI_FFI_OK;
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
    protocolVersion = 54;
    commandResult = KODOSI_FFI_OK;
}
}
