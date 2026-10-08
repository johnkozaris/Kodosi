#include "RuntimeApiFixture.hpp"
extern "C" {
void* kodosi_start(const kodosi_callbacks_t* callbacks, uintptr_t, void* userdata)
{
    runtime_fixture::callbacks = *callbacks;
    runtime_fixture::userdata = userdata;
    return runtime_fixture::allowStart ? reinterpret_cast<void*>(1) : nullptr;
}
void kodosi_stop(void*)
{
    runtime_fixture::callbacks = {};
    runtime_fixture::userdata = nullptr;
}
uint32_t kodosi_abi_version()
{
    return 7;
}
uint32_t kodosi_protocol_version()
{
    return runtime_fixture::protocolVersion;
}
int32_t kodosi_send_command(void*, const uint8_t* bytes, uintptr_t size)
{
    runtime_fixture::lastCommand
        = QByteArray(reinterpret_cast<const char*>(bytes), static_cast<qsizetype>(size));
    return runtime_fixture::commandResult;
}
int32_t kodosi_terminal_input(
    void*, const char*, const char*, const char*, uint64_t, const uint8_t*, uintptr_t)
{
    return KODOSI_FFI_RUNTIME_STOPPED;
}
int32_t kodosi_terminal_connect(void*, const char*, const char*, uint64_t)
{
    return KODOSI_FFI_RUNTIME_STOPPED;
}
int32_t kodosi_terminal_refresh(void*, const char*, const char*, uint64_t)
{
    return KODOSI_FFI_RUNTIME_STOPPED;
}
int32_t kodosi_terminal_disconnect(void*, const char*, const char*, uint64_t)
{
    return KODOSI_FFI_RUNTIME_STOPPED;
}
}
