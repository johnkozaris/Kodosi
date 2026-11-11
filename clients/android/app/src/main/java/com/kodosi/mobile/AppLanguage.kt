package com.kodosi.mobile

import android.content.Context

object AppLanguage {
    fun tag(context: Context): String = context.resources.configuration.locales[0].toLanguageTag()
}
