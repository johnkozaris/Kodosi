package com.kodosi.mobile.canvas

import android.media.MediaCodecList
import android.media.MediaFormat
import androidx.compose.ui.unit.IntSize
import com.kodosi.mobile.runtime.fields
import java.io.ByteArrayOutputStream
import kotlinx.serialization.json.JsonObject

// The codec byte of a frame's header, and the codec's name in the browser's messages.
internal enum class VideoCodec(val wire: Int, val mime: String) {
    H264(0, MediaFormat.MIMETYPE_VIDEO_AVC),
    HEVC(1, MediaFormat.MIMETYPE_VIDEO_HEVC);

    // The size of the pictures from a keyframe's sequence parameter set.
    fun pictureSize(frame: ByteArray): IntSize? {
        var index = 3
        while (index < frame.size) {
            if (frame[index - 1] == 1.toByte() && frame[index - 2] == 0.toByte() && frame[index - 3] == 0.toByte()) {
                val header = frame[index].toInt()
                val size = when {
                    this == H264 && (header and 0x1f) == H264_SEQUENCE ->
                        runCatching { Bits(payload(frame, index + 1)).h264Size() }

                    this == HEVC && ((header shr 1) and 0x3f) == HEVC_SEQUENCE ->
                        runCatching { Bits(payload(frame, index + 2)).hevcSize() }

                    else -> null
                }
                if (size != null) return size.getOrNull()?.takeIf { it.width > 0 && it.height > 0 }
            }
            index++
        }
        return null
    }

    companion object {
        fun of(wire: Int) = entries.firstOrNull { it.wire == wire }

        // The codecs this device decodes and which of them in hardware, so the host streams in
        // the best codec that every viewer decodes.
        val decodable: List<JsonObject> by lazy {
            val decoders = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { !it.isEncoder }
            entries.mapNotNull { codec ->
                val able = decoders.filter { codec.mime in it.supportedTypes }
                if (able.isEmpty()) {
                    null
                } else {
                    fields("codec" to codec.name.lowercase(), "hardware" to able.any { it.isHardwareAccelerated })
                }
            }
        }
    }
}

private const val H264_SEQUENCE = 7
private const val HEVC_SEQUENCE = 33
private const val GENERAL_PROFILE_TIER_LEVEL = 96
private const val SUB_LAYER_PROFILE = 88
private val profilesWithChromaFormat = setOf(100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135)

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

// Parses SPS fields per ITU-T H.264 section 7.3.2.1.1.
private fun Bits.h264Size(): IntSize {
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

// Parses SPS fields per ITU-T H.265 section 7.3.2.2.1.
private fun Bits.hevcSize(): IntSize {
    bits(4)
    val subLayers = bits(3)
    flag()
    skip(GENERAL_PROFILE_TIER_LEVEL)
    val present = List(subLayers) { flag() to flag() }
    if (subLayers > 0) skip(2 * (8 - subLayers))
    for ((profile, level) in present) {
        if (profile) skip(SUB_LAYER_PROFILE)
        if (level) skip(8)
    }
    ue()
    val chroma = ue()
    if (chroma == 3) flag()
    val width = ue()
    val height = ue()
    val crop = if (flag()) IntArray(4) { ue() } else IntArray(4)
    val cropX = if (chroma == 1 || chroma == 2) 2 else 1
    val cropY = if (chroma == 1) 2 else 1
    return IntSize(width - cropX * (crop[0] + crop[1]), height - cropY * (crop[2] + crop[3]))
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

    fun skip(count: Int) {
        position += count
    }
}
