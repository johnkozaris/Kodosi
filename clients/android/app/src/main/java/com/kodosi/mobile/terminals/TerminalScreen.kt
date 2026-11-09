package com.kodosi.mobile.terminals

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.Glyph
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.raised

/** A terminal of another computer, live, in a dark tile, with the keys that a phone lacks. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TerminalScreen(model: AppModel, sessionId: String, back: () -> Unit) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val connection = remember(sessionId) { model.connection(sessionId, scope) } ?: return
    val catalog by model.terminals.state.collectAsStateWithLifecycle()
    val state by connection.state.collectAsStateWithLifecycle()
    val session = catalog.sessions.firstOrNull { it.id == sessionId }
    var view by remember { mutableStateOf<TerminalView?>(null) }
    var control by remember { mutableStateOf(false) }
    DisposableEffect(connection) { onDispose { connection.close() } }
    LifecycleResumeEffect(connection) {
        connection.show(true)
        onPauseOrDispose { connection.show(false) }
    }
    LaunchedEffect(catalog) { connection.refresh() }

    val typing = WindowInsets.isImeVisible
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    Column(Modifier.fillMaxSize().background(colors.surface).statusBarsPadding().imePadding()) {
        if (!(typing && landscape)) Header(session, back)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 10.dp)
                .raised(colors, RoundedCornerShape(Radius.tile), fill = colors.terminal)
                .clip(RoundedCornerShape(Radius.tile))
        ) {
            AndroidView(
                factory = { context ->
                    TerminalView(context, connection).also { created ->
                        created.onControlChanged = { control = it }
                        view = created
                    }
                },
                modifier = Modifier.fillMaxSize().padding(8.dp)
            )
            KodosiTheme(dark = true) {
                when {
                    session == null -> StateCard(stringResource(R.string.terminal_ended), null, working = false) {
                        KodosiButton(
                            stringResource(R.string.back_to_terminals),
                            onClick = back,
                            kind = ButtonKind.SECONDARY
                        )
                    }

                    !session.isConnected -> SessionState(session, connection)

                    else -> RenderState(state, connection)
                }
            }
        }
        if (typing) {
            KeyRow(view, control)
        } else {
            Row(
                Modifier.fillMaxWidth().navigationBarsPadding().padding(10.dp),
                horizontalArrangement = Arrangement.End
            ) {
                KeyCap(stringResource(R.string.keyboard), enabled = state == TerminalConnection.State.Rendered) {
                    view?.showKeyboard()
                }
            }
        }
    }
}

@Composable
private fun Header(session: RuntimeSession?, back: () -> Unit) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 6.dp)
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = back)
        ) {
            Glyph(R.drawable.ic_back, colors.ink, 22.dp, description = stringResource(R.string.back))
        }
        if (session != null) {
            AgentMark(
                session.agent,
                28.dp,
                account = session.programAccount,
                activity = if (session.isWorking) MarkActivity.WORKING else MarkActivity.AWAKE
            )
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        session.name,
                        style = KodosiType.headline,
                        color = colors.ink,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    session.sign()?.let {
                        Spacer(Modifier.width(6.dp))
                        StatusSign(it, 13.dp)
                    }
                }
                TerminalCaption(session)
            }
        }
    }
}

@Composable
private fun SessionState(session: RuntimeSession, connection: TerminalConnection) {
    when {
        session.connectionState == RuntimeSession.Connection.BLOCKED -> StateCard(
            statusLabel(session),
            session.message,
            working = false
        )

        session.status == RuntimeSession.Status.OFFLINE ->
            StateCard(statusLabel(session), stringResource(R.string.terminal_opens_when_back), working = false)

        session.connectionState == RuntimeSession.Connection.OFFLINE -> StateCard(
            statusLabel(session),
            session.message,
            working = false
        ) {
            if (session.canOpen) KodosiButton(stringResource(R.string.connect), onClick = connection::retry)
        }

        else -> StateCard(stringResource(R.string.connecting), null, working = true)
    }
}

@Composable
private fun RenderState(state: TerminalConnection.State, connection: TerminalConnection) {
    when (state) {
        TerminalConnection.State.Rendered -> Unit

        TerminalConnection.State.Connecting -> StateCard(stringResource(R.string.connecting), null, working = true)

        is TerminalConnection.State.Closed -> StateCard(
            stringResource(R.string.terminal_not_available),
            state.reason ?: stringResource(R.string.terminal_closed),
            working = false
        ) { KodosiButton(stringResource(R.string.try_again), onClick = connection::retry) }

        is TerminalConnection.State.Failed -> StateCard(
            stringResource(R.string.terminal_not_available),
            state.message,
            working = false
        ) {
            KodosiButton(stringResource(R.string.try_again), onClick = connection::retry)
        }
    }
}

@Composable
private fun StateCard(title: String, message: String?, working: Boolean, action: @Composable () -> Unit = {}) {
    val colors = KodosiTheme.colors
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        modifier = Modifier.fillMaxSize().background(colors.terminal).padding(24.dp)
    ) {
        if (working) {
            ProgressCaption(title)
        } else {
            Text(title, style = KodosiType.headline, color = colors.ink, textAlign = TextAlign.Center)
        }
        if (message != null) {
            Text(
                message,
                style = KodosiType.footnote,
                color = colors.inkMuted,
                textAlign = TextAlign.Center,
                modifier = Modifier.widthIn(max = 320.dp)
            )
        }
        action()
    }
}

/** The keys that a phone keyboard lacks, on the keyboard while you type in the terminal. */
@Composable
private fun KeyRow(view: TerminalView?, control: Boolean) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().horizontalScroll(
            rememberScrollState()
        ).padding(horizontal = 10.dp, vertical = 6.dp)
    ) {
        KeyCap("esc", stringResource(R.string.key_escape)) { view?.press(TerminalKernel.Key.ESCAPE) }
        KeyCap("tab", stringResource(R.string.key_tab)) { view?.press(TerminalKernel.Key.TAB) }
        KeyCap("ctrl", stringResource(R.string.key_control), on = control) { view?.let { it.control = !it.control } }
        KeyCap("←", stringResource(R.string.key_left)) { view?.press(TerminalKernel.Key.LEFT) }
        KeyCap("↑", stringResource(R.string.key_up)) { view?.press(TerminalKernel.Key.UP) }
        KeyCap("↓", stringResource(R.string.key_down)) { view?.press(TerminalKernel.Key.DOWN) }
        KeyCap("→", stringResource(R.string.key_right)) { view?.press(TerminalKernel.Key.RIGHT) }
    }
}

@Composable
private fun KeyCap(cap: String, label: String = cap, on: Boolean = false, enabled: Boolean = true, press: () -> Unit) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(Radius.sm)
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .heightIn(min = 48.dp)
            .widthIn(min = 48.dp)
            .raised(colors, shape, fill = if (on) colors.accentSoft else colors.lifted)
            .clip(shape)
            .clickable(enabled = enabled, role = Role.Button, onClick = press)
            .semantics { contentDescription = label }
            .padding(horizontal = 12.dp)
    ) {
        Text(
            cap,
            style = KodosiType.subhead,
            color = when {
                on -> colors.accent
                enabled -> colors.ink
                else -> colors.inkFaint
            }
        )
    }
}
