package com.kodosi.mobile.chat

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TalkVoiceTest {
    @Test
    fun aVoiceStandsOutFromSteadySoundAndSteadySoundIsNoVoiceEvenWhenItIsLoud() {
        val voice = TalkVoice()
        var time = 0L
        fun sound(decibels: Double): Boolean {
            time += 100
            return voice.hears(decibels, time)
        }

        assertTrue((0 until 30).none { sound(-45.0) })
        assertTrue(sound(-20.0))
        assertFalse(sound(-46.0))
        assertFalse((0 until 200).map { sound(-20.0) }.last())
    }
}
