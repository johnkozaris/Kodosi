package com.kodosi.mobile

import android.app.Application
import android.content.ComponentCallbacks2
import android.content.res.Configuration
import androidx.compose.runtime.Composer
import androidx.compose.runtime.tooling.ComposeStackTraceMode
import com.kodosi.mobile.notifications.FirebaseConfiguration
import com.kodosi.mobile.terminals.TerminalNotifications

class KodosiApplication : Application() {
    val preferences: AppPreferences by lazy { AppPreferences(this) }
    val notifications: TerminalNotifications by lazy { TerminalNotifications(this, preferences) }
    private val made = lazy { AppModel(this, preferences, notifications) }
    val model: AppModel by made

    override fun onCreate() {
        super.onCreate()
        Composer.setDiagnosticStackTraceMode(ComposeStackTraceMode.Auto)
        FirebaseConfiguration.initialize(this)
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        model.languageChanged()
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_BACKGROUND && made.isInitialized()) model.video.trim()
    }
}
