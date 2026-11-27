package com.kodosi.mobile.runtime

import android.util.Log

class RuntimeEventFaults {
    private var last: String? = null

    fun unreadable(type: String?, reason: String) =
        once("the app could not read ${type?.let { "the event $it" } ?: "an event"}: $reason")

    fun unreadableChatFrame(reason: String) = once("the app could not read a frame of a terminal's chat: $reason")

    private fun once(fault: String) {
        if (fault == last) return
        last = fault
        write(Log.ERROR, Ffi.LOG_ERROR, fault)
    }

    fun behind(events: Int, bytes: Long) = write(
        Log.WARN,
        Ffi.LOG_WARNING,
        "the app fell behind the runtime's events and dropped $events of them, $bytes bytes, " +
            "so it asks for its state again"
    )

    fun video(act: String, result: Int) =
        write(Log.WARN, Ffi.LOG_WARNING, "the runtime refused to $act a video pipe with result $result")

    private fun write(priority: Int, level: Int, line: String) {
        Log.println(priority, TAG, line)
        NativeRuntime.log(level, line.encodeToByteArray())
    }

    private companion object {
        const val TAG = "Kodosi"
    }
}
