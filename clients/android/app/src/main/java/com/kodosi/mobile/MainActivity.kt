package com.kodosi.mobile

import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.net.toUri
import com.kodosi.mobile.ui.KodosiTheme

class MainActivity : ComponentActivity() {
    private val model: AppModel get() = (application as KodosiApplication).model
    private var signingIn = false

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        model.start()
        receive(intent)
        setContent { KodosiTheme { App(model, openSignIn = ::openSignIn) } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        receive(intent)
    }

    override fun onResume() {
        super.onResume()
        if (signingIn) {
            signingIn = false
            model.account.cancelSignIn()
            return
        }
        model.refresh()
    }

    private fun receive(intent: Intent) {
        val answer = intent.data?.takeIf { it.scheme == SIGN_IN_SCHEME } ?: return
        signingIn = false
        model.account.finishSignIn(answer.toString())
    }

    private fun openSignIn(page: String) {
        model.account.openedSignInPage()
        try {
            CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(this, page.toUri())
            signingIn = true
        } catch (failure: ActivityNotFoundException) {
            model.account.failSignIn(getString(R.string.sign_in_page_failed))
        }
    }

    private companion object {
        const val SIGN_IN_SCHEME = "com.kodosi"
    }
}
