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
int32_t kodosi_last_start_failure(kodosi_start_failure_t* out)
{
    if (!runtime_fixture::startFailure)
        return 0;
    *out = *runtime_fixture::startFailure;
    return 1;
}
int32_t kodosi_host_stop(int32_t)
{
    ++runtime_fixture::hostStops;
    return KODOSI_HOST_STOP_ACCEPTED;
}
uint32_t kodosi_abi_version()
{
    return KODOSI_FFI_ABI_VERSION;
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
