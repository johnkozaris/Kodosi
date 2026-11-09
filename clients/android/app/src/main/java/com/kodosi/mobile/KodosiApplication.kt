package com.kodosi.mobile

import android.app.Application
import android.content.res.Configuration

class KodosiApplication : Application() {
    val model: AppModel by lazy { AppModel(this) }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        model.languageChanged()
    }
}
