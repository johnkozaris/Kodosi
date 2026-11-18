package com.kodosi.mobile.canvas

import androidx.compose.ui.unit.IntSize
import java.io.ByteArrayOutputStream

internal object H264 {
    private const val SEQUENCE_PARAMETER_SET = 7
    private val profilesWithChromaFormat = setOf(100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135)

    fun pictureSize(frame: ByteArray): IntSize? {
        var index = 3
        while (index < frame.size) {
            if (frame[index - 1] == 1.toByte() && frame[index - 2] == 0.toByte() && frame[index - 3] == 0.toByte() &&
                (frame[index].toInt() and 0x1f) == SEQUENCE_PARAMETER_SET
            ) {
                return runCatching { Bits(payload(frame, index + 1)).sequenceSize() }.getOrNull()
                    ?.takeIf { it.width > 0 && it.height > 0 }
            }
            index++
        }
        return null
    }

    private fun payload(frame: ByteArray, from: Int): ByteArray {
        val payload = ByteArrayOutputStream()
        var zeros = 0
        for (index in from until frame.size) {
            val byte = frame[index].toInt() and 0xff
            if (zeros >= 2 && byte < 3) break
            if (zeros < 2 || byte != 3) payload.write(byte)
            zeros = if (byte == 0) zeros + 1 else 0
        }
        return payload.toByteArray()
    }

    /** The fields follow section 7.3.2.1.1 of H.264. */
    private fun Bits.sequenceSize(): IntSize {
        val profile = bits(8)
        bits(16)
        ue()
        var chroma = 1
        var separatePlanes = false
        if (profile in profilesWithChromaFormat) {
            chroma = ue()
            if (chroma == 3) separatePlanes = flag()
            ue()
            ue()
            flag()
            if (flag()) repeat(if (chroma == 3) 12 else 8) { if (flag()) skipScalingList(if (it < 6) 16 else 64) }
        }
        ue()
        when (ue()) {
            0 -> ue()

            1 -> {
                flag()
                se()
                se()
                repeat(ue()) { se() }
            }
        }
        ue()
        flag()
        val width = (ue() + 1) * 16
        val mapUnits = ue() + 1
        val framesOnly = flag()
        if (!framesOnly) flag()
        flag()
        val crop = if (flag()) IntArray(4) { ue() } else IntArray(4)
        val subsampled = chroma != 0 && !separatePlanes
        val cropX = if (subsampled && chroma != 3) 2 else 1
        val cropY = (if (framesOnly) 1 else 2) * (if (subsampled && chroma == 1) 2 else 1)
        return IntSize(
            width - cropX * (crop[0] + crop[1]),
            mapUnits * (if (framesOnly) 16 else 32) - cropY * (crop[2] + crop[3])
        )
    }

    private fun Bits.skipScalingList(size: Int) {
        var last = 8
        var next = 8
        repeat(size) {
            if (next != 0) next = (last + se() + 256) % 256
            if (next != 0) last = next
        }
    }

    private class Bits(private val bytes: ByteArray) {
        private var position = 0

        fun bits(count: Int): Int {
            var value = 0
            repeat(count) {
                val bit = (bytes[position shr 3].toInt() shr (7 - (position and 7))) and 1
                value = (value shl 1) or bit
                position++
            }
            return value
        }

        fun flag() = bits(1) == 1

        fun ue(): Int {
            var zeros = 0
            while (!flag()) require(++zeros < 31)
            return (1 shl zeros) - 1 + bits(zeros)
        }

        fun se(): Int = ue().let { if (it % 2 == 1) (it + 1) / 2 else -(it / 2) }
    }
}
