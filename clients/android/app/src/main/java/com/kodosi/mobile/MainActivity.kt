package com.kodosi.mobile

import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Bundle
import android.view.View
import android.view.WindowInsetsController
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.browser.auth.AuthTabIntent
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.core.net.toUri
import androidx.core.view.doOnAttach
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import com.kodosi.mobile.account.DeviceCode
import com.kodosi.mobile.account.SignInStage
import com.kodosi.mobile.people.PeopleStore
import com.kodosi.mobile.terminals.TerminalNotifications
import com.kodosi.mobile.ui.KodosiTheme
import kotlinx.coroutines.launch

private const val KODOSI_SCHEME = "kodosi"

class MainActivity : ComponentActivity() {
    private val model: AppModel get() = (application as KodosiApplication).model
    private val signIn = AuthTabIntent.registerActivityResultLauncher(this) { result ->
        if (result.resultCode == AuthTabIntent.RESULT_OK && result.resultUri?.scheme == SIGN_IN_SCHEME) {
            model.account.finishSignIn(result.resultUri.toString())
        } else if (model.account.state.value.stage is SignInStage.AwaitingBrowser) {
            if (result.resultCode == AuthTabIntent.RESULT_CANCELED) {
                model.account.cancelSignIn()
            } else {
                model.account.failSignIn(getString(R.string.sign_in_page_failed))
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        model.start()
        receive(intent)
        setContent {
            val preferences by model.preferences.state.collectAsStateWithLifecycle()
            val dark = when (preferences.appearance) {
                "dark" -> true
                "light" -> false
                else -> isSystemInDarkTheme()
            }
            SideEffect {
                val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                    WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
                window.decorView.doOnAttach { view ->
                    view.windowInsetsController?.setSystemBarsAppearance(if (dark) 0 else light, light)
                }
            }
            KodosiTheme(dark) { App(model, openSignIn = ::openSignIn) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        receive(intent)
    }

    override fun onResume() {
        super.onResume()
        model.lock.foreground()
        privacy(background = false)
        lifecycleScope.launch { model.lock.unlock(this@MainActivity, automatic = true) }
        model.languageChanged()
        model.refresh()
        model.updatePush()
    }

    override fun onStart() {
        super.onStart()
        model.notifications.foreground = true
    }

    override fun onPause() {
        privacy(background = true)
        super.onPause()
    }

    override fun onStop() {
        model.notifications.foreground = false
        model.lock.background()
        super.onStop()
    }

    private fun privacy(background: Boolean) {
        val locked = model.preferences.state.value.lock
        setRecentsScreenshotEnabled(!locked)
        window.decorView.contentSensitivity = if (background && locked) {
            View.CONTENT_SENSITIVITY_SENSITIVE
        } else {
            View.CONTENT_SENSITIVITY_AUTO
        }
    }

    private fun receive(intent: Intent) {
        val opened = intent.data?.takeIf { it.scheme == KODOSI_SCHEME }?.toString()
        opened?.let(PeopleStore::invite)?.let { invite ->
            model.friendInvite.value = invite
            intent.data = null
        }
        opened?.let(DeviceCode::join)?.let { code ->
            model.account.joinDevice(code)
            intent.data = null
        }
        val terminal = intent.getStringExtra(TerminalNotifications.TERMINAL)
        val account = intent.getStringExtra(TerminalNotifications.ACCOUNT)
        if (terminal != null && account != null) {
            model.terminalTarget.value = AppModel.TerminalTarget(terminal, account)
            intent.removeExtra(TerminalNotifications.TERMINAL)
            intent.removeExtra(TerminalNotifications.ACCOUNT)
        }
    }

    private fun openSignIn(page: String) {
        model.account.openedSignInPage()
        try {
            AuthTabIntent.Builder().build().launch(signIn, page.toUri(), SIGN_IN_SCHEME)
        } catch (failure: ActivityNotFoundException) {
            model.account.failSignIn(getString(R.string.sign_in_page_failed))
        }
    }
}
