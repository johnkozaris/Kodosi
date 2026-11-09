package com.kodosi.mobile

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.onboarding.OnboardingScreen
import com.kodosi.mobile.terminals.TerminalScreen
import com.kodosi.mobile.terminals.TerminalsScreen
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Wordmark

@Composable
fun App(model: AppModel, openSignIn: (String) -> Unit) {
    val colors = KodosiTheme.colors
    val phase by model.phase.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val notices = remember { SnackbarHostState() }
    LaunchedEffect(model) { model.notices.collect { notices.showSnackbar(it) } }
    Box(Modifier.fillMaxSize().background(colors.surface)) {
        when (val current = phase) {
            AppModel.Phase.Launching -> Launch()

            is AppModel.Phase.Failed -> Failure(current.message, model::retry)

            AppModel.Phase.Ready -> when {
                account.isReady -> Home(model)
                account.authResolved -> OnboardingScreen(model.account, openSignIn)
                else -> Launch()
            }
        }
        SnackbarHost(notices, Modifier.align(Alignment.BottomCenter).navigationBarsPadding())
    }
}

@Composable
private fun Home(model: AppModel) {
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    val terminal = open
    if (terminal == null) {
        TerminalsScreen(model.terminals, open = { open = it })
    } else {
        BackHandler { open = null }
        TerminalScreen(model, terminal, back = { open = null })
    }
}

@Composable
private fun Launch() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Wordmark(height = 30.dp) }
}

@Composable
private fun Failure(message: String, retry: () -> Unit) {
    val colors = KodosiTheme.colors
    Column(
        verticalArrangement = Arrangement.spacedBy(20.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.fillMaxSize().padding(32.dp)
    ) {
        Wordmark(height = 24.dp)
        Text(
            message,
            style = KodosiType.callout,
            color = colors.inkMuted,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 440.dp)
        )
        KodosiButton(stringResource(R.string.try_again), onClick = retry)
    }
}
