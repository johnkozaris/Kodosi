package com.kodosi.mobile

import android.content.Context

/** The language that the app shows, as the runtime and the sign-in pages name it. */
object AppLanguage {
    fun tag(context: Context): String = context.resources.configuration.locales[0].toLanguageTag()
}
