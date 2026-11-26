package com.kodosi.mobile.chat

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
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
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.AgentSettingsSection
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalFace
import com.kodosi.mobile.terminals.changesAgent
import com.kodosi.mobile.terminals.rememberAgentModels
import com.kodosi.mobile.ui.LinkOpener
import com.kodosi.mobile.ui.LocalLinkOpener
import kotlinx.coroutines.launch

/**
 * A terminal's chat, shown in place of its screen on the app's surface. The chat's pipe stays open
 * while the chat is visible, links in it follow the rules of the terminal's computer, and its rows
 * keep the name of the agent that wrote them after that agent ends.
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
        // Dismiss the keyboard on a new prompt so the ask card has enough vertical space.
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
    val submit: (String) -> Boolean = { text ->
        chat.submit(text).also { submitted ->
            if (submitted) {
                sent++
                haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            }
        }
    }
    val talk = rememberChatTalk(chat, session, view, submit) { words ->
        draft.setTextAndPlaceCursorAtEnd(joinedWords(draft.text.toString().trimEnd(), words))
        // Talk mode also ends as the chat view leaves, when the draft no longer follows its field.
        chat.draft = draft.text.toString()
    }
    CompositionLocalProvider(LocalLinkOpener provides opener, LocalProseFolder provides view.facts.folder) {
        Box(modifier, contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 760.dp).fillMaxSize()) {
                val writer = AgentKind.of(view.facts.agent).takeIf { it.isAgent } ?: session.agent
                ChatThread(view, writer, account.userId, sent, hideKeyboard, Modifier.weight(1f).fillMaxWidth())
                ChatDock(
                    chat,
                    view,
                    session.agent,
                    draft,
                    talk,
                    known,
                    send = { if (submit(draft.text.toString())) draft.clearText() },
                    showTerminal = {
                        hideKeyboard()
                        model.terminals.pick(TerminalFace.TERMINAL, session.id, session.face)
                    },
                    changesAgent = session.changesAgent()
                ) { AgentSettingsSection(model, session) }
            }
        }
    }
}
