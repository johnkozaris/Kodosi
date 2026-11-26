package com.kodosi.mobile.chat

/**
 * Tells a voice from the steady sound around the speaker, such as a fan or a street, by how far the
 * sound rises above it. Recognized words arrive late, so this is what shows that a person still
 * talks.
 */
internal class TalkVoice {
    /**
     * The steady sound, in decibels: it drops at once to quieter sound and rises slowly, so the
     * short gaps between words keep it down while a person talks.
     */
    private var floor: Double? = null
    private var lastSound: Long? = null

    /** Takes the loudness of the latest sound and returns whether it is a voice. */
    fun hears(decibels: Double, atMillis: Long): Boolean {
        val elapsed = lastSound?.let { (atMillis - it) / 1000.0 } ?: 0.0
        lastSound = atMillis
        val steady = floor
        if (steady == null) {
            floor = decibels
            return false
        }
        floor = minOf(decibels, steady + RISE * elapsed)
        return decibels > maxOf(steady, QUIETEST) + MARGIN
    }

    private companion object {
        const val RISE = 3.0
        const val MARGIN = 12.0
        const val QUIETEST = -60.0
    }
}
