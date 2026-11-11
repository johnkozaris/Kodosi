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
static jobject receiver;
static uint8_t empty_payload;

static void detach(void *attached) {
    (void)attached;
    (*java)->DetachCurrentThread(java);
}

// The runtime calls back on its own threads, which stay attached until they end.
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
    (*env)->DeleteLocalRef(env, callbacks);
    if (on_event == NULL || on_terminal_data == NULL || on_terminal_control == NULL ||
        on_terminal_connect_result == NULL || on_terminal_checkpoint == NULL) {
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

// An ABI bridge can give the VM and the Rust library different libc environments.
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

JNIEXPORT jint JNICALL Java_com_kodosi_mobile_runtime_NativeRuntime_terminalRefresh(JNIEnv *env, jclass type,
                                                                                    jlong handle, jstring session,
                                                                                    jstring subscription,
                                                                                    jlong generation) {
    (void)type;
    return call_terminal(env, kodosi_terminal_refresh, handle, session, subscription, generation);
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
