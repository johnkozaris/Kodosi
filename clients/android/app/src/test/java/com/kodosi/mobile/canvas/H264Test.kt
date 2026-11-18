package com.kodosi.mobile.canvas

import androidx.compose.ui.unit.IntSize
import org.junit.Assert.assertEquals
import org.junit.Test

class H264Test {
    @Test
    fun readsTheCroppedPictureSizeOfEachProfileFromAKeyFrame() {
        val sizes = mapOf(
            "2742c00c898a08861bc23cdc0808081e10084230" to IntSize(160, 96),
            "2742c032898a180220093f2cf35020202078402119" to IntSize(1080, 2340),
            "2742c034898a30048004034d40808081e10084d0" to IntSize(2304, 4096),
            "27640020ac1314501581879b9a81010103c2010960" to IntSize(1366, 768)
        )
        for ((sequence, size) in sizes) {
            assertEquals(size, H264.pictureSize(bytes("00000001$sequence" + "0000000128ce3c80" + "0000000125b80040")))
        }
    }

    private fun bytes(hex: String) = hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
