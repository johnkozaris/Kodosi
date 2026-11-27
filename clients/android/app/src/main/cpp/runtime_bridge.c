#include <jni.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "kodosi_runtime.h"

static JavaVM *java;
static pthread_key_t attachment;
static jmethodID on_event;
static jmethodID on_terminal_data;
static jmethodID on_terminal_control;
static jmethodID on_terminal_connect_result;
static jmethodID on_terminal_checkpoint;
static jmethodID on_video_frame;
static jmethodID on_video_message;
static jmethodID on_video_repair;
static jobject receiver;
static uint8_t empty_payload;

static void detach(void *attached) {
    (void)attached;
    (*java)->DetachCurrentThread(java);
}

// Runtime callbacks run on Rust threads, each watched video pipe's own among them; keep each thread attached until it
// exits.
static JNIEnv *attach(void) {
    JNIEnv *env = NULL;
    if ((*java)->GetEnv(java, (void **)&env, JNI_VERSION_1_6) == JNI_OK) {
        return env;
    }
    if ((*java)->AttachCurrentThreadAsDaemon(java, &env, NULL) != JNI_OK) {
        return NULL;
    }
    pthread_setspecific(attachment, env);
    return env;
}

static bool settle(JNIEnv *env) {
    if (!(*env)->ExceptionCheck(env)) {
        return true;
    }
    (*env)->ExceptionClear(env);
    return false;
}

static jbyteArray byte_array(JNIEnv *env, const uint8_t *bytes, uintptr_t length) {
    jbyteArray array = (*env)->NewByteArray(env, (jsize)length);
    if (array != NULL && length > 0) {
        (*env)->SetByteArrayRegion(env, array, 0, (jsize)length, (const jbyte *)bytes);
    }
    return array;
}

// A direct buffer over the runtime's bytes, valid only during the callback.
static jobject borrowed(JNIEnv *env, const uint8_t *bytes, uintptr_t length) {
    void *address = length == 0 ? &empty_payload : (void *)bytes;
    return (*env)->NewDirectByteBuffer(env, address, (jlong)length);
}

static void event_callback(const uint8_t *json, uintptr_t length, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 4) != JNI_OK) {
        return;
    }
    jbyteArray array = byte_array(env, json, length);
    if (array != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_event, array);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

static void data_callback(const char *session, const char *subscription, uint64_t generation, uint64_t sequence,
                          const uint8_t *bytes, uintptr_t length, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 8) != JNI_OK) {
        return;
    }
    jstring session_id = (*env)->NewStringUTF(env, session);
    jstring subscription_id = (*env)->NewStringUTF(env, subscription);
    jobject buffer = borrowed(env, bytes, length);
    if (session_id != NULL && subscription_id != NULL && buffer != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_terminal_data, session_id, subscription_id, (jlong)generation,
                               (jlong)sequence, buffer);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

static void control_callback(const char *session, const char *subscription, uint64_t generation, const uint8_t *json,
                             uintptr_t length, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 8) != JNI_OK) {
        return;
    }
    jstring session_id = (*env)->NewStringUTF(env, session);
    jstring subscription_id = (*env)->NewStringUTF(env, subscription);
    jbyteArray array = byte_array(env, json, length);
    if (session_id != NULL && subscription_id != NULL && array != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_terminal_control, session_id, subscription_id,
                               (jlong)generation, array);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

static void connect_result_callback(const char *session, const char *subscription, uint64_t generation, int32_t result,
                                    void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 4) != JNI_OK) {
        return;
    }
    jstring session_id = (*env)->NewStringUTF(env, session);
    jstring subscription_id = (*env)->NewStringUTF(env, subscription);
    if (session_id != NULL && subscription_id != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_terminal_connect_result, session_id, subscription_id,
                               (jlong)generation, (jint)result);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

static int32_t checkpoint_callback(const char *session, const char *subscription, uint64_t generation,
                                   uint64_t next_sequence, uint16_t rows, uint16_t cols, bool console,
                                   const uint8_t *snapshot, uintptr_t length, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 8) != JNI_OK) {
        return KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    }
    jint answer = KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    jstring session_id = (*env)->NewStringUTF(env, session);
    jstring subscription_id = (*env)->NewStringUTF(env, subscription);
    jobject buffer = borrowed(env, snapshot, length);
    if (session_id != NULL && subscription_id != NULL && buffer != NULL) {
        answer = (*env)->CallIntMethod(env, (jobject)userdata, on_terminal_checkpoint, session_id, subscription_id,
                                       (jlong)generation, (jlong)next_sequence, (jint)rows, (jint)cols,
                                       (jboolean)console, buffer);
    }
    int32_t result = settle(env) ? answer : KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED;
    (*env)->PopLocalFrame(env, NULL);
    return result;
}

static int32_t video_frame_callback(const char *pipe, const kodosi_video_frame_t *frame, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 4) != JNI_OK) {
        return KODOSI_VIDEO_FAILED;
    }
    jint answer = KODOSI_VIDEO_FAILED;
    jstring pipe_id = (*env)->NewStringUTF(env, pipe);
    jobject buffer = borrowed(env, frame->data, frame->length);
    jfloatArray region = NULL;
    if (frame->region != NULL) {
        const jfloat part[4] = {frame->region->x, frame->region->y, frame->region->width, frame->region->height};
        region = (*env)->NewFloatArray(env, 4);
        if (region != NULL) {
            (*env)->SetFloatArrayRegion(env, region, 0, 4, part);
        }
    }
    if (pipe_id != NULL && buffer != NULL && (frame->region == NULL || region != NULL)) {
        answer = (*env)->CallIntMethod(env, (jobject)userdata, on_video_frame, pipe_id, (jint)frame->codec,
                                       (jint)frame->kind, region, buffer);
    }
    int32_t result = settle(env) && answer == KODOSI_VIDEO_TAKEN ? KODOSI_VIDEO_TAKEN : KODOSI_VIDEO_FAILED;
    (*env)->PopLocalFrame(env, NULL);
    return result;
}

static void video_message_callback(const char *pipe, const uint8_t *json, uintptr_t length, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 4) != JNI_OK) {
        return;
    }
    jstring pipe_id = (*env)->NewStringUTF(env, pipe);
    jbyteArray array = byte_array(env, json, length);
    if (pipe_id != NULL && array != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_video_message, pipe_id, array);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

static void video_repair_callback(const char *pipe, void *userdata) {
    JNIEnv *env = attach();
    if (env == NULL || (*env)->PushLocalFrame(env, 2) != JNI_OK) {
        return;
    }
    jstring pipe_id = (*env)->NewStringUTF(env, pipe);
    if (pipe_id != NULL) {
        (*env)->CallVoidMethod(env, (jobject)userdata, on_video_repair, pipe_id);
    }
    settle(env);
    (*env)->PopLocalFrame(env, NULL);
}

JNIEXPORT jint JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    java = vm;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK || pthread_key_create(&attachment, detach) != 0) {
        return JNI_ERR;
    }
    jclass callbacks = (*env)->FindClass(env, "com/kodosi/mobile/runtime/RuntimeCallbacks");
    if (callbacks == NULL) {
        return JNI_ERR;
    }
    on_event = (*env)->GetMethodID(env, callbacks, "onEvent", "([B)V");
    on_terminal_data = (*env)->GetMethodID(env, callbacks, "onTerminalData",
                                           "(Ljava/lang/String;Ljava/lang/String;JJLjava/nio/ByteBuffer;)V");
    on_terminal_control =
        (*env)->GetMethodID(env, callbacks, "onTerminalControl", "(Ljava/lang/String;Ljava/lang/String;J[B)V");
    on_terminal_connect_result =
        (*env)->GetMethodID(env, callbacks, "onTerminalConnectResult", "(Ljava/lang/String;Ljava/lang/String;JI)V");
    on_terminal_checkpoint = (*env)->GetMethodID(env, callbacks, "onTerminalCheckpoint",
                                                 "(Ljava/lang/String;Ljava/lang/String;JJIIZLjava/nio/ByteBuffer;)I");
    on_video_frame =
        (*env)->GetMethodID(env, callbacks, "onVideoFrame", "(Ljava/lang/String;II[FLjava/nio/ByteBuffer;)I");
    on_video_message = (*env)->GetMethodID(env, callbacks, "onVideoMessage", "(Ljava/lang/String;[B)V");
    on_video_repair = (*env)->GetMethodID(env, callbacks, "onVideoRepair", "(Ljava/lang/String;)V");
    (*env)->DeleteLocalRef(env, callbacks);
    if (on_event == NULL || on_terminal_data == NULL || on_terminal_control == NULL ||
        on_terminal_connect_result == NULL || on_terminal_checkpoint == NULL || on_video_frame == NULL ||
        on_video_message == NULL || on_video_repair == NULL) {
        return JNI_ERR;
    }
    return JNI_VERSION_1_6;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_abiVersion(JNIEnv *env, jclass type) {
    (void)env;
    (void)type;
    return (jint)kodosi_abi_version();
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_protocolVersion(JNIEnv *env, jclass type) {
    (void)env;
    (void)type;
    return (jint)kodosi_protocol_version();
}

// Set in native code because an ABI bridge can give the JVM and Rust separate libc environments.
JNIEXPORT jboolean JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_setEnvironment(JNIEnv *env, jclass type,
                                                                                      jstring name, jbyteArray value) {
    (void)type;
    const char *key = (*env)->GetStringUTFChars(env, name, NULL);
    if (key == NULL) {
        return JNI_FALSE;
    }
    jsize length = (*env)->GetArrayLength(env, value);
    char *bytes = malloc((size_t)length + 1);
    if (bytes == NULL) {
        (*env)->ReleaseStringUTFChars(env, name, key);
        return JNI_FALSE;
    }
    (*env)->GetByteArrayRegion(env, value, 0, length, (jbyte *)bytes);
    bool installed = false;
    if (!(*env)->ExceptionCheck(env) && memchr(bytes, '\0', (size_t)length) == NULL) {
        bytes[length] = '\0';
        installed = setenv(key, bytes, 1) == 0;
    }
    free(bytes);
    (*env)->ReleaseStringUTFChars(env, name, key);
    return installed ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_prepare(JNIEnv *env, jclass type, jobject context) {
    (void)type;
    return kodosi_android_init(env, context);
}

JNIEXPORT void JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_setLanguage(JNIEnv *env, jclass type,
                                                                                jstring language) {
    (void)type;
    const char *tag = (*env)->GetStringUTFChars(env, language, NULL);
    if (tag == NULL) {
        return;
    }
    kodosi_set_language(tag);
    (*env)->ReleaseStringUTFChars(env, language, tag);
}

JNIEXPORT jlong JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_start(JNIEnv *env, jclass type,
                                                                           jobject callbacks) {
    (void)type;
    if (receiver != NULL) {
        return 0;
    }
    receiver = (*env)->NewGlobalRef(env, callbacks);
    kodosi_callbacks_t table = {
        .on_event = event_callback,
        .on_terminal_data = data_callback,
        .on_terminal_control = control_callback,
        .on_terminal_connect_result = connect_result_callback,
        .on_terminal_checkpoint = checkpoint_callback,
        .on_video_frame = video_frame_callback,
        .on_video_message = video_message_callback,
        .on_video_repair = video_repair_callback,
    };
    void *handle = kodosi_start(&table, sizeof(table), receiver);
    if (handle == NULL) {
        (*env)->DeleteGlobalRef(env, receiver);
        receiver = NULL;
    }
    return (jlong)(intptr_t)handle;
}

JNIEXPORT void JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_stop(JNIEnv *env, jclass type, jlong handle) {
    (void)type;
    kodosi_stop((void *)(intptr_t)handle);
    if (receiver != NULL) {
        (*env)->DeleteGlobalRef(env, receiver);
        receiver = NULL;
    }
}

JNIEXPORT jstring JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_lastStartFailure(JNIEnv *env, jclass type) {
    (void)type;
    kodosi_start_failure_t failure;
    memset(&failure, 0, sizeof(failure));
    if (kodosi_last_start_failure(&failure) != 1) {
        return NULL;
    }
    failure.message[KODOSI_START_FAILURE_MESSAGE_BYTES - 1] = '\0';
    return (*env)->NewStringUTF(env, failure.message);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_sendCommand(JNIEnv *env, jclass type, jlong handle,
                                                                                jbyteArray json) {
    (void)type;
    jsize length = (*env)->GetArrayLength(env, json);
    jbyte *bytes = (*env)->GetByteArrayElements(env, json, NULL);
    if (bytes == NULL) {
        return KODOSI_FFI_DESER_FAILED;
    }
    int32_t result = kodosi_send_command((void *)(intptr_t)handle, (const uint8_t *)bytes, (uintptr_t)length);
    (*env)->ReleaseByteArrayElements(env, json, bytes, JNI_ABORT);
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_log(JNIEnv *env, jclass type, jint level,
                                                                        jbyteArray line) {
    (void)type;
    jsize length = (*env)->GetArrayLength(env, line);
    jbyte *bytes = (*env)->GetByteArrayElements(env, line, NULL);
    if (bytes == NULL) {
        return KODOSI_FFI_DESER_FAILED;
    }
    int32_t result = kodosi_log((int32_t)level, (const uint8_t *)bytes, (uintptr_t)length);
    (*env)->ReleaseByteArrayElements(env, line, bytes, JNI_ABORT);
    return result;
}

JNIEXPORT jbyteArray JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_linkDecision(JNIEnv *env, jclass type,
                                                                                       jbyteArray link,
                                                                                       jbyteArray computer) {
    (void)type;
    jsize link_length = (*env)->GetArrayLength(env, link);
    jsize computer_length = (*env)->GetArrayLength(env, computer);
    jbyte *link_bytes = (*env)->GetByteArrayElements(env, link, NULL);
    jbyte *computer_bytes = (*env)->GetByteArrayElements(env, computer, NULL);
    jbyteArray result = NULL;
    if (link_bytes != NULL && computer_bytes != NULL) {
        uint8_t first[4096];
        uint8_t *decision = first;
        int64_t capacity = (int64_t)sizeof first;
        int64_t length = kodosi_link_decision((const uint8_t *)link_bytes, (uintptr_t)link_length,
                                              (const uint8_t *)computer_bytes, (uintptr_t)computer_length, decision,
                                              (uintptr_t)capacity);
        if (length > capacity && length <= INT32_MAX) {
            decision = malloc((size_t)length);
            capacity = decision == NULL ? 0 : length;
            if (decision != NULL) {
                length = kodosi_link_decision((const uint8_t *)link_bytes, (uintptr_t)link_length,
                                              (const uint8_t *)computer_bytes, (uintptr_t)computer_length, decision,
                                              (uintptr_t)capacity);
            }
        }
        if (decision != NULL && length >= 0 && length <= capacity) {
            result = (*env)->NewByteArray(env, (jsize)length);
            if (result != NULL) {
                (*env)->SetByteArrayRegion(env, result, 0, (jsize)length, (const jbyte *)decision);
            }
        }
        if (decision != first) {
            free(decision);
        }
    }
    if (link_bytes != NULL) {
        (*env)->ReleaseByteArrayElements(env, link, link_bytes, JNI_ABORT);
    }
    if (computer_bytes != NULL) {
        (*env)->ReleaseByteArrayElements(env, computer, computer_bytes, JNI_ABORT);
    }
    return result;
}

typedef int32_t (*terminal_call)(void *, const char *, const char *, uint64_t);

static jint call_terminal(JNIEnv *env, terminal_call call, jlong handle, jstring session, jstring subscription,
                          jlong generation) {
    const char *session_id = (*env)->GetStringUTFChars(env, session, NULL);
    const char *subscription_id = (*env)->GetStringUTFChars(env, subscription, NULL);
    jint result = KODOSI_FFI_DESER_FAILED;
    if (session_id != NULL && subscription_id != NULL) {
        result = call((void *)(intptr_t)handle, session_id, subscription_id, (uint64_t)generation);
    }
    if (session_id != NULL) {
        (*env)->ReleaseStringUTFChars(env, session, session_id);
    }
    if (subscription_id != NULL) {
        (*env)->ReleaseStringUTFChars(env, subscription, subscription_id);
    }
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_terminalConnect(JNIEnv *env, jclass type,
                                                                                    jlong handle, jstring session,
                                                                                    jstring subscription,
                                                                                    jlong generation) {
    (void)type;
    return call_terminal(env, kodosi_terminal_connect, handle, session, subscription, generation);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_terminalDisconnect(JNIEnv *env, jclass type,
                                                                                       jlong handle, jstring session,
                                                                                       jstring subscription,
                                                                                       jlong generation) {
    (void)type;
    return call_terminal(env, kodosi_terminal_disconnect, handle, session, subscription, generation);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_terminalInput(JNIEnv *env, jclass type,
                                                                                  jlong handle, jstring session,
                                                                                  jstring incarnation,
                                                                                  jstring subscription,
                                                                                  jlong generation, jbyteArray input) {
    (void)type;
    const char *session_id = (*env)->GetStringUTFChars(env, session, NULL);
    const char *incarnation_id = (*env)->GetStringUTFChars(env, incarnation, NULL);
    const char *subscription_id = (*env)->GetStringUTFChars(env, subscription, NULL);
    jsize length = (*env)->GetArrayLength(env, input);
    jbyte *bytes = (*env)->GetByteArrayElements(env, input, NULL);
    jint result = KODOSI_FFI_DESER_FAILED;
    if (session_id != NULL && incarnation_id != NULL && subscription_id != NULL && bytes != NULL) {
        result = kodosi_terminal_input((void *)(intptr_t)handle, session_id, incarnation_id, subscription_id,
                                       (uint64_t)generation, (const uint8_t *)bytes, (uintptr_t)length);
    }
    if (bytes != NULL) {
        (*env)->ReleaseByteArrayElements(env, input, bytes, JNI_ABORT);
    }
    if (subscription_id != NULL) {
        (*env)->ReleaseStringUTFChars(env, subscription, subscription_id);
    }
    if (incarnation_id != NULL) {
        (*env)->ReleaseStringUTFChars(env, incarnation, incarnation_id);
    }
    if (session_id != NULL) {
        (*env)->ReleaseStringUTFChars(env, session, session_id);
    }
    return result;
}

typedef int32_t (*video_call)(void *, const char *);

static jint call_video(JNIEnv *env, video_call call, jlong handle, jstring pipe) {
    const char *pipe_id = (*env)->GetStringUTFChars(env, pipe, NULL);
    if (pipe_id == NULL) {
        return KODOSI_FFI_DESER_FAILED;
    }
    jint result = call((void *)(intptr_t)handle, pipe_id);
    (*env)->ReleaseStringUTFChars(env, pipe, pipe_id);
    return result;
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_videoWatch(JNIEnv *env, jclass type, jlong handle,
                                                                               jstring pipe) {
    (void)type;
    return call_video(env, kodosi_video_watch, handle, pipe);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_videoRestart(JNIEnv *env, jclass type,
                                                                                 jlong handle, jstring pipe) {
    (void)type;
    return call_video(env, kodosi_video_restart, handle, pipe);
}

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_videoUnwatch(JNIEnv *env, jclass type,
                                                                                 jlong handle, jstring pipe) {
    (void)type;
    return call_video(env, kodosi_video_unwatch, handle, pipe);
}
