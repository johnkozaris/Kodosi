package com.kodosi.mobile.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.R
import com.kodosi.mobile.account.AccountStore
import com.kodosi.mobile.account.DeviceTrust
import com.kodosi.mobile.account.SignInStage
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.Glyph
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.Wordmark
import com.kodosi.mobile.ui.raised

@Composable
fun OnboardingScreen(account: AccountStore, openSignIn: (String) -> Unit) {
    val state by account.state.collectAsStateWithLifecycle()
    val haptics = LocalHapticFeedback.current
    LaunchedEffect(state.signInPage) { state.signInPage?.let(openSignIn) }
    LaunchedEffect(state.stage is SignInStage.Failed) {
        if (state.stage is SignInStage.Failed) haptics.performHapticFeedback(HapticFeedbackType.Reject)
    }
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.fillMaxSize().safeDrawingPadding().imePadding().verticalScroll(rememberScrollState())
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(32.dp),
            modifier = Modifier.widthIn(max = 440.dp).padding(horizontal = 24.dp, vertical = 32.dp)
        ) {
            Wordmark(height = 30.dp)
            AnimatedContent(
                targetState = state.stage,
                transitionSpec = { fadeIn() togetherWith fadeOut() },
                contentKey = ::stageKey,
                label = "sign-in stage"
            ) { stage ->
                when (stage) {
                    SignInStage.Idle, SignInStage.SignedIn -> Welcome(account, failure = null, starting = false)

                    SignInStage.Starting -> Welcome(account, failure = null, starting = true)

                    is SignInStage.Failed -> Welcome(account, failure = stage.message, starting = false)

                    is SignInStage.AwaitingBrowser -> Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(22.dp)
                    ) {
                        ProgressCaption(stringResource(R.string.waiting_for_sign_in_page))
                        KodosiButton(
                            stringResource(R.string.cancel),
                            onClick = account::cancelSignIn,
                            kind = ButtonKind.GHOST
                        )
                    }

                    SignInStage.Finalizing -> ProgressCaption(stringResource(R.string.signing_in))

                    is SignInStage.TrustingDevice -> TrustThisPhone(account, stage.trust)
                }
            }
        }
    }
}

private fun stageKey(stage: SignInStage): String = when (stage) {
    SignInStage.Idle, SignInStage.Starting, SignInStage.SignedIn, is SignInStage.Failed -> "welcome"
    is SignInStage.AwaitingBrowser -> "browser"
    SignInStage.Finalizing -> "finalizing"
    is SignInStage.TrustingDevice -> "trust"
}

@Composable
private fun Welcome(account: AccountStore, failure: String?, starting: Boolean) {
    val colors = KodosiTheme.colors
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(28.dp)) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                stringResource(R.string.welcome_title),
                style = KodosiType.large,
                color = colors.ink,
                textAlign = TextAlign.Center
            )
            Text(
                stringResource(R.string.welcome_message),
                style = KodosiType.callout,
                color = colors.inkMuted,
                textAlign = TextAlign.Center
            )
        }
        Column(
            verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier.fillMaxWidth().raised(colors, RoundedCornerShape(Radius.xl)).padding(20.dp)
        ) {
            Feature(stringResource(R.string.feature_follow_title), stringResource(R.string.feature_follow_detail)) {
                AgentMark(AgentKind.CLAUDE, 34.dp, activity = MarkActivity.WORKING)
            }
            Feature(stringResource(R.string.feature_answer_title), stringResource(R.string.feature_answer_detail)) {
                Box {
                    AgentMark(AgentKind.CODEX, 34.dp)
                    StatusSign(StatusForm.HAND, 11.dp, Modifier.align(Alignment.TopEnd).padding(start = 30.dp))
                }
            }
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (failure != null) ErrorNote(failure, Modifier.fillMaxWidth())
            KodosiButton(
                stringResource(if (starting) R.string.opening_sign_in else R.string.sign_in),
                onClick = account::signIn,
                size = com.kodosi.mobile.ui.ButtonSize.LARGE,
                enabled = !starting,
                modifier = Modifier.fillMaxWidth()
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Glyph(R.drawable.ic_lock, colors.inkFaint, 12.dp)
                Spacer(Modifier.width(5.dp))
                Text(stringResource(R.string.encrypted_end_to_end), style = KodosiType.caption, color = colors.inkFaint)
            }
        }
    }
}

@Composable
private fun Feature(title: String, detail: String, mark: @Composable () -> Unit) {
    val colors = KodosiTheme.colors
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(40.dp), contentAlignment = Alignment.CenterStart) { mark() }
        Spacer(Modifier.width(14.dp))
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = KodosiType.headline, color = colors.ink)
            Text(detail, style = KodosiType.footnote, color = colors.inkMuted)
        }
    }
}

@Composable
private fun TrustThisPhone(account: AccountStore, trust: DeviceTrust) {
    val colors = KodosiTheme.colors
    var confirmingReset by remember { mutableStateOf(false) }
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp)) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Text(
                stringResource(R.string.trust_title),
                style = KodosiType.title,
                color = colors.ink,
                textAlign = TextAlign.Center
            )
            Text(
                stringResource(R.string.trust_message),
                style = KodosiType.callout,
                color = colors.inkMuted,
                textAlign = TextAlign.Center
            )
        }
        when (trust) {
            DeviceTrust.Choose -> TrustChoices(account, reset = { confirmingReset = true })

            DeviceTrust.Requesting -> ProgressCaption(stringResource(R.string.asking_other_devices))

            is DeviceTrust.PendingApproval -> Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(18.dp)
            ) {
                Text(
                    stringResource(R.string.approval_instructions),
                    style = KodosiType.callout,
                    color = colors.inkMuted,
                    textAlign = TextAlign.Center
                )
                VerificationCode(trust.code)
                ProgressCaption(stringResource(R.string.waiting_for_approval))
                KodosiButton(
                    stringResource(R.string.cancel_request),
                    onClick = account::cancelDeviceApproval,
                    kind = ButtonKind.GHOST
                )
            }

            DeviceTrust.Recovering -> ProgressCaption(stringResource(R.string.checking_recovery_key))

            DeviceTrust.Resetting -> ProgressCaption(stringResource(R.string.starting_fresh))

            is DeviceTrust.Waiting -> Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                Text(
                    stringResource(
                        R.string.start_fresh_wait,
                        java.text.DateFormat.getDateTimeInstance().format(java.util.Date(trust.readyAtMs))
                    ),
                    style = KodosiType.callout,
                    color = colors.inkMuted
                )
                KodosiButton(
                    stringResource(R.string.cancel_request),
                    onClick = account::stopReset,
                    kind = ButtonKind.GHOST
                )
            }

            is DeviceTrust.Failed -> Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(14.dp)
            ) {
                ErrorNote(trust.message, Modifier.fillMaxWidth())
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    KodosiButton(
                        stringResource(R.string.sign_in_again),
                        onClick = account::restartSignIn,
                        kind = ButtonKind.SECONDARY
                    )
                    KodosiButton(stringResource(R.string.try_again), onClick = account::chooseTrust)
                }
            }
        }
    }
    if (confirmingReset) {
        KodosiDialog(
            onDismissRequest = { confirmingReset = false },
            title = { Text(stringResource(R.string.start_fresh_title)) },
            text = { Text(stringResource(R.string.start_fresh_message)) },
            confirmButton = {
                KodosiTextButton(onClick = {
                    confirmingReset = false
                    account.startFresh()
                }) { Text(stringResource(R.string.start_fresh), color = colors.danger) }
            },
            dismissButton = {
                KodosiTextButton(onClick = { confirmingReset = false }) { Text(stringResource(R.string.cancel)) }
            }
        )
    }
}

@Composable
private fun TrustChoices(account: AccountStore, reset: () -> Unit) {
    var entering by rememberSaveable { mutableStateOf(false) }
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        KodosiButton(
            stringResource(R.string.approve_from_another_device),
            onClick = account::requestDeviceApproval,
            size = com.kodosi.mobile.ui.ButtonSize.LARGE,
            modifier = Modifier.fillMaxWidth()
        )
        if (entering) {
            RecoveryKeyField(account::useRecoveryKey)
        } else {
            KodosiButton(
                stringResource(R.string.use_recovery_key),
                onClick = { entering = true },
                kind = ButtonKind.SECONDARY,
                size = com.kodosi.mobile.ui.ButtonSize.LARGE,
                modifier = Modifier.fillMaxWidth()
            )
        }
        KodosiButton(stringResource(R.string.no_other_device), onClick = reset, kind = ButtonKind.GHOST)
    }
}

@Composable
private fun RecoveryKeyField(recover: (String) -> Unit) {
    var key by rememberSaveable { mutableStateOf("") }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        KodosiField(
            value = key,
            onValueChange = { key = it },
            singleLine = true,
            placeholder = stringResource(R.string.recovery_key),
            textStyle = KodosiType.callout.copy(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.Characters,
                autoCorrectEnabled = false,
                imeAction = ImeAction.Go
            ),
            keyboardActions = KeyboardActions(onGo = { if (key.isNotBlank()) recover(key) }),
            modifier = Modifier.fillMaxWidth()
        )
        KodosiButton(stringResource(R.string.approve), onClick = { recover(key) }, enabled = key.isNotBlank())
    }
}

@Composable
private fun VerificationCode(code: String) {
    val colors = KodosiTheme.colors
    Text(
        code,
        style = KodosiType.title.copy(fontFamily = FontFamily.Monospace),
        color = colors.ink,
        modifier = Modifier.raised(colors, RoundedCornerShape(Radius.lg)).padding(horizontal = 22.dp, vertical = 14.dp)
    )
}
