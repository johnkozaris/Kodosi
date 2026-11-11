package com.kodosi.mobile

import android.app.LocaleManager
import android.content.Context
import android.os.LocaleList

/** The language that the app shows, as the runtime and the sign-in pages name it. */
object AppLanguage {
    fun tag(context: Context): String = context.resources.configuration.locales[0].toLanguageTag()

    fun selected(context: Context): String =
        context.getSystemService(LocaleManager::class.java).applicationLocales[0]?.toLanguageTag().orEmpty()

    fun choose(context: Context, language: String) {
        context.getSystemService(LocaleManager::class.java).applicationLocales = LocaleList.forLanguageTags(language)
    }
}
