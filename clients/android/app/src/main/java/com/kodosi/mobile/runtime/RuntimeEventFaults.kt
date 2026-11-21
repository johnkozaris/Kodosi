package com.kodosi.mobile.runtime

import android.util.Log

/**
 * An event that the app cannot read, and the events that it drops when it falls behind, go to
 * Android's log and into the runtime's own log, beside the lines of the runtime. The app goes
 * on without them.
 */
class RuntimeEventFaults {
    private var last: String? = null

    fun unreadable(type: String?, reason: String) =
        once("the app could not read ${type?.let { "the event $it" } ?: "an event"}: $reason")

    fun unreadableChatFrame(reason: String) = once("the app could not read a frame of a terminal's chat: $reason")

    /** A fault that repeats with each new event or frame goes into the log once. */
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

    private fun write(priority: Int, level: Int, line: String) {
        Log.println(priority, TAG, line)
        NativeRuntime.log(level, line.encodeToByteArray())
    }

    private companion object {
        const val TAG = "Kodosi"
    }
}
