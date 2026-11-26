package com.kodosi.mobile.canvas

import android.content.Context
import android.net.wifi.WifiManager

// Keeps Wi-Fi out of its power saving while video arrives, which otherwise holds packets back for a
// beacon interval or longer. Android honours the lock only while the app is in front with the screen on.
class StreamingWifi(context: Context) {
    private val lock = context.getSystemService(WifiManager::class.java)
        ?.createWifiLock(WifiManager.WIFI_MODE_FULL_LOW_LATENCY, "Kodosi video")
        ?.apply { setReferenceCounted(false) }

    fun keep(on: Boolean) {
        runCatching { if (on) lock?.acquire() else lock?.release() }
    }
}
