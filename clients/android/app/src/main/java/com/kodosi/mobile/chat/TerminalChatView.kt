package com.kodosi.mobile.chat

import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentSettingsSection
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalFace
import com.kodosi.mobile.terminals.changesAgent
import com.kodosi.mobile.terminals.rememberAgentModels
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LinkOpener
import com.kodosi.mobile.ui.LocalLinkOpener
import kotlinx.coroutines.launch

/**
 * The chat of a terminal in the place of its screen, on the app's surface. The pipe of the chat is
 * open while the chat shows, and a link in it follows the rules of the terminal's computer.
 */
@Composable
fun TerminalChatView(model: AppModel, session: RuntimeSession, modifier: Modifier = Modifier) {
    val chat = remember(session.id) { model.canvas.chat(session.id) }
    val view by chat.view.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    LifecycleResumeEffect(chat) {
        val watching = scope.launch { chat.watch() }
        onPauseOrDispose { watching.cancel() }
    }
    val draft = rememberSaveable(session.id, saver = TextFieldState.Saver) { TextFieldState(chat.draft) }
    LaunchedEffect(draft) { snapshotFlow { draft.text.toString() }.collect { chat.draft = it } }
    var sent by rememberSaveable(session.id) { mutableIntStateOf(0) }
    val haptics = LocalHapticFeedback.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val hideKeyboard: () -> Unit = {
        focus.clearFocus()
        keyboard?.hide()
    }
    val ask = view.asks.firstOrNull()?.id
    var seen by remember { mutableStateOf(ask) }
    LaunchedEffect(ask) {
        // The keyboard of a phone would leave a question that comes little room.
        if (ask != null && ask != seen) {
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
            if (draft.text.isBlank()) hideKeyboard()
        }
        seen = ask
    }
    val agentModels = rememberAgentModels(model, session, load = session.agent.isAgent)
    val known = KnownModels(
        label = { id -> ChatWords.modelName(id, agentModels.models) },
        levels = agentModels::levels
    )
    val links = LocalLinkOpener.current
    val opener = remember(links, session.hostName) {
        LinkOpener { address, _ -> links.open(address, session.hostName) }
    }
    CompositionLocalProvider(LocalLinkOpener provides opener, LocalProseFolder provides view.facts.folder) {
        Box(modifier, contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 760.dp).fillMaxSize()) {
                ChatThread(view, session.agent, account.userId, sent, hideKeyboard, Modifier.weight(1f).fillMaxWidth())
                ChatDock(
                    chat,
                    view,
                    session.agent,
                    draft,
                    known,
                    send = {
                        if (chat.submit(draft.text.toString())) {
                            draft.clearText()
                            sent++
                            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                        }
                    },
                    showTerminal = {
                        hideKeyboard()
                        model.terminals.pick(TerminalFace.TERMINAL, session.id, session.face)
                    }
                ) { label, open, setOpen -> ModelKey(model, session, label, open, setOpen) }
            }
        }
    }
}

/**
 * The model in a chat's facts opens the agent's part of the terminal's actions, which changes its
 * model and which `/model` also opens.
 */
@Composable
private fun ModelKey(
    model: AppModel,
    session: RuntimeSession,
    label: String,
    open: Boolean,
    setOpen: (Boolean) -> Unit
) {
    val colors = KodosiTheme.colors
    val changes = session.changesAgent()
    Box {
        Row(
            Modifier.heightIn(min = 48.dp).then(
                if (changes) {
                    Modifier.clickable(
                        remember { MutableInteractionSource() },
                        indication = null,
                        role = Role.Button,
                        onClickLabel = stringResource(R.string.model)
                    ) { setOpen(true) }
                } else {
                    Modifier
                }
            ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                label,
                style = KodosiType.footnote,
                color = colors.inkMuted,
                maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis,
                modifier = Modifier.widthIn(max = 180.dp)
            )
            if (changes) {
                Spacer(Modifier.width(4.dp))
                AppIcon(AppSymbol.CHEVRONS, Modifier.size(8.dp), tint = colors.inkFaint)
            }
        }
        KodosiMenu(open && changes, { setOpen(false) }) { AgentSettingsSection(model, session) }
    }
}
