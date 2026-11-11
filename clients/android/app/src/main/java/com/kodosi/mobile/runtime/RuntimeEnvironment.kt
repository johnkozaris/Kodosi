package com.kodosi.mobile.runtime

import android.content.Context
import android.os.Build
import android.provider.Settings
import android.system.Os
import com.kodosi.mobile.BuildConfig
import java.io.File

/** Gives the runtime its settings, which it reads from the environment when it starts. */
object RuntimeEnvironment {
    fun install(context: Context) {
        val root = File(context.noBackupFilesDir, "kodosi")
        check(root.isDirectory || root.mkdirs())
        Os.chmod(root.path, PRIVATE)
        Os.setenv("KODOSI_PRODUCTION_DATA_ROOT", root.path, true)
        Os.setenv("KODOSI_DEVICE_LABEL", deviceName(context), true)
        if (BuildConfig.DEBUG) {
            BuildConfig.DEBUG_BACKEND_API.takeIf {
                it.isNotEmpty()
            }?.let { Os.setenv("KODOSI__BACKEND__API", it, true) }
            BuildConfig.DEBUG_AUTH_ISSUER.takeIf {
                it.isNotEmpty()
            }?.let { Os.setenv("KODOSI__AUTH__ISSUER", it, true) }
        }
    }

    private fun deviceName(context: Context): String =
        Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)?.takeIf { it.isNotBlank() }
            ?: "${Build.MANUFACTURER} ${Build.MODEL}"

    private const val PRIVATE = 0b111_000_000
}
