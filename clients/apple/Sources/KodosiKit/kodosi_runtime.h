#ifndef KODOSI_RUNTIME_H
#define KODOSI_RUNTIME_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define KODOSI_FFI_ABI_VERSION 7

#define KODOSI_MAX_FRAME_BYTES 8388608

#define KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES 8388608

#define KODOSI_TERMINAL_CONTROL_MAX_BYTES 65536

#define KODOSI_FFI_OK 0

#define KODOSI_FFI_NULL_HANDLE 2

#define KODOSI_FFI_DESER_FAILED 3

#define KODOSI_FFI_PAYLOAD_TOO_LARGE 4

#define KODOSI_FFI_RUNTIME_STOPPED 5

#define KODOSI_FFI_BUSY 6

#define KODOSI_FFI_SESSION_NOT_FOUND 7

#define KODOSI_FFI_STALE_SUBSCRIPTION 8

#define KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED 9

#define KODOSI_FFI_PANIC -1

#define KODOSI_START_INVALID_CALLBACKS 1

#define KODOSI_START_ALREADY_ACTIVE 2

#define KODOSI_START_HOST_BUSY 3

#define KODOSI_START_REJECTED 4

#define KODOSI_START_FAILED 5

#define KODOSI_START_FAILURE_MESSAGE_BYTES 512

#define KODOSI_HOST_KIND_UNKNOWN 0

#define KODOSI_HOST_KIND_APP 1

#define KODOSI_HOST_KIND_FOREGROUND 2

#define KODOSI_HOST_KIND_BACKGROUND 3

#define KODOSI_HOST_STOP_ACCEPTED 0

#define KODOSI_HOST_STOP_REFUSED 1

#define KODOSI_HOST_STOP_UNREACHABLE 2

#define KODOSI_HOST_STOP_FAILED 3

#define KODOSI_CLI_NOT_INVOKED -1

#define KODOSI_CLI_FAILED 70

typedef struct kodosi_start_failure_t {
  int32_t code;
  int32_t host_kind;
  uint32_t host_pid;
  uint32_t host_local_sessions;
  char message[KODOSI_START_FAILURE_MESSAGE_BYTES];
} kodosi_start_failure_t;

typedef void (*kodosi_event_cb_t)(const uint8_t*, uintptr_t, void*);

typedef void (*kodosi_terminal_data_cb_t)(const char*,
                                          const char*,
                                          uint64_t,
                                          uint64_t,
                                          const uint8_t*,
                                          uintptr_t,
                                          void*);

typedef void (*kodosi_terminal_control_cb_t)(const char*,
                                             const char*,
                                             uint64_t,
                                             const uint8_t*,
                                             uintptr_t,
                                             void*);

typedef void (*kodosi_terminal_connect_result_cb_t)(const char*,
                                                    const char*,
                                                    uint64_t,
                                                    int32_t,
                                                    void*);

typedef int32_t (*kodosi_terminal_checkpoint_cb_t)(const char*,
                                                   const char*,
                                                   uint64_t,
                                                   uint64_t,
                                                   uint16_t,
                                                   uint16_t,
                                                   const uint8_t*,
                                                   uintptr_t,
                                                   void*);

typedef struct kodosi_callbacks_t {
  kodosi_event_cb_t on_event;
  kodosi_terminal_data_cb_t on_terminal_data;
  kodosi_terminal_control_cb_t on_terminal_control;
  kodosi_terminal_connect_result_cb_t on_terminal_connect_result;
  kodosi_terminal_checkpoint_cb_t on_terminal_checkpoint;
} kodosi_callbacks_t;

#ifdef __cplusplus
extern "C" {
#endif

 void kodosi_stop(void *handle) ;

 uint32_t kodosi_abi_version(void) ;

 uint32_t kodosi_protocol_version(void) ;

 int32_t kodosi_last_start_failure(struct kodosi_start_failure_t *out) ;

 int32_t kodosi_host_stop(int32_t force) ;

 int32_t kodosi_cli_main(int32_t length, const char *const *values) ;

 int32_t kodosi_send_command(void *handle, const uint8_t *bytes, uintptr_t len) ;


void *kodosi_start(const struct kodosi_callbacks_t *callbacks,
                   uintptr_t callbacks_size,
                   void *userdata)
;


int32_t kodosi_terminal_input(void *handle,
                              const char *session_id,
                              const char *expected_runtime_incarnation_id,
                              const char *subscription_id,
                              uint64_t subscription_generation,
                              const uint8_t *bytes,
                              uintptr_t len)
;


int32_t kodosi_terminal_connect(void *handle,
                                const char *session_id,
                                const char *subscription_id,
                                uint64_t generation)
;


int32_t kodosi_terminal_refresh(void *handle,
                                const char *session_id,
                                const char *subscription_id,
                                uint64_t generation)
;


int32_t kodosi_terminal_disconnect(void *handle,
                                   const char *session_id,
                                   const char *subscription_id,
                                   uint64_t generation)
;

#ifdef __cplusplus
}
#endif

#endif
