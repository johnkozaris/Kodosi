package com.kodosi.mobile.runtime

import android.util.Log

/**
 * An event that the app cannot read, and the events that it drops when it falls behind, go to
 * Android's log and into the runtime's own log, beside the lines of the runtime. The app goes
 * on without them.
 */
class RuntimeEventFaults {
    private var last: String? = null

    /** A fault that repeats with each new event goes into the log once. */
    fun unreadable(type: String?, reason: String) {
        val fault = "the app could not read ${type?.let { "the event $it" } ?: "an event"}: $reason"
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

    fun browserBehind(events: Int, bytes: Long) = write(
        Log.WARN,
        Ffi.LOG_WARNING,
        "the shared browser fell behind the runtime's events and dropped $events of them, $bytes bytes, " +
            "so its pipes start again"
    )

    private fun write(priority: Int, level: Int, line: String) {
        Log.println(priority, TAG, line)
        NativeRuntime.log(level, line.encodeToByteArray())
    }

    private companion object {
        const val TAG = "Kodosi"
    }
}
