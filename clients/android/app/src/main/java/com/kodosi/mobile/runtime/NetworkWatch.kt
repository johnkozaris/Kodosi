package com.kodosi.mobile.runtime

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.os.Handler
import android.os.Looper

// Android does not expose network transitions directly to the Rust runtime, so relay them here.
class NetworkWatch(context: Context, private val onChange: () -> Unit) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private val settled = Runnable { onChange() }
    private var current: Network? = null
    private var watching = false
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            main.post { use(network) }
        }

        override fun onLost(network: Network) {
            main.post { if (network == current) use(null) }
        }
    }

    fun watch(on: Boolean) {
        if (on == watching) return
        watching = on
        if (on) {
            current = connectivity.activeNetwork
            connectivity.registerDefaultNetworkCallback(callback)
        } else {
            connectivity.unregisterNetworkCallback(callback)
            main.removeCallbacks(settled)
        }
    }

    private fun use(network: Network?) {
        if (!watching || network == current) return
        current = network
        main.removeCallbacks(settled)
        main.postDelayed(settled, 1_000)
    }
}
