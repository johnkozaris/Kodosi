package com.kodosi.mobile.terminals

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.defaultMinSize
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.canvas.readPickedFile
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.FileIcon
import com.kodosi.mobile.ui.Glyph
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListButton
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.MessageComposer
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.TextEntryDialog
import com.kodosi.mobile.ui.bezel
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.terminalTile
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** A terminal of another computer, live, in a dark tile, with the keys that a phone lacks. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TerminalScreen(model: AppModel, sessionId: String, online: Boolean, back: () -> Unit, canvas: (String?) -> Unit) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val connection = remember(sessionId) { model.connection(sessionId, scope) } ?: return
    val catalog by model.terminals.state.collectAsStateWithLifecycle()
    val state by connection.state.collectAsStateWithLifecycle()
    val session = catalog.sessions.firstOrNull { it.id == sessionId }
    var view by remember { mutableStateOf<TerminalView?>(null) }
    var control by remember { mutableStateOf(false) }
    val preferences by model.preferences.state.collectAsStateWithLifecycle()
    val news by model.canvas.news.collectAsStateWithLifecycle()
    var composing by rememberSaveable(sessionId) { mutableStateOf(false) }
    var draft by rememberSaveable(sessionId) { mutableStateOf("") }
    var renaming by remember { mutableStateOf(false) }
    var closing by remember { mutableStateOf(false) }
    var clipboardAnswer by remember { mutableStateOf<((String) -> Boolean)?>(null) }
    val context = LocalContext.current
    val links = com.kodosi.mobile.ui.LocalLinkOpener.current
    val keyboard = LocalSoftwareKeyboardController.current
    val focus = LocalFocusManager.current
    val composerFocus = remember { FocusRequester() }
    DisposableEffect(connection) { onDispose { connection.close() } }
    LifecycleResumeEffect(connection) {
        model.notifications.showing(sessionId)
        connection.show(true)
        onPauseOrDispose {
            model.notifications.showing(null)
            connection.show(false)
        }
    }
    LaunchedEffect(catalog) { connection.refresh() }
    DisposableEffect(view, preferences.keepScreenOn) {
        view?.keepScreenOn = preferences.keepScreenOn
        onDispose { view?.keepScreenOn = false }
    }
    LifecycleResumeEffect(composing, view) {
        connection.inputFocus(!composing)
        val terminal = view
        terminal?.isFocusableInTouchMode = !composing
        val inputFocus = Runnable {
            if (composing) {
                terminal?.clearFocus()
                composerFocus.requestFocus()
            } else {
                terminal?.requestFocus()
            }
        }
        terminal?.post(inputFocus)
        onPauseOrDispose {
            terminal?.removeCallbacks(inputFocus)
            terminal?.isFocusableInTouchMode = false
            terminal?.clearFocus()
        }
    }

    val typing = WindowInsets.isImeVisible
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val textSize = (if (LocalConfiguration.current.smallestScreenWidthDp >= 600) 14f else 12f) + preferences.textSize
    Column(Modifier.fillMaxSize().background(colors.surface).statusBarsPadding().imePadding()) {
        if (!(typing && landscape)) {
            Row(Modifier.padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) {
                    Header(session, online, model.account::retryConnection, back, rename = { renaming = true })
                }
                if (typing && !landscape && !composing) {
                    ActionIcon(
                        AppSymbol.COMPOSE,
                        R.string.compose,
                        { composing = true },
                        enabled = session?.canControl == true
                    )
                }
                if (session?.canvas != null) ActionIcon(AppSymbol.CANVAS, R.string.canvas, { canvas(null) })
                TerminalActions(model, session, view, back, canvas, { renaming = true }, { closing = true })
            }
        }
        news[sessionId]?.let { key ->
            session?.let { terminal ->
                CanvasNotice(terminal, key, open = { canvas(key) }, dismiss = { model.canvas.saw(terminal) })
            }
        }
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 10.dp)
                .terminalTile(colors)
                .clip(RoundedCornerShape(Radius.tile))
        ) {
            AndroidView(
                factory = { context ->
                    TerminalView(context, connection, scope).also { created ->
                        created.onControlChanged = { control = it }
                        created.onClipboardRead = { clipboardAnswer = it }
                        created.onFailure = { model.notify(model.words.didNotWork) }
                        view = created
                    }
                },
                update = {
                    it.fontSize(textSize)
                    it.onLink = { address -> links.open(address, session?.hostName) }
                    it.onPastePicture = { uri ->
                        val target = session
                        if (target?.canControl == true) {
                            scope.launch {
                                model.runAction {
                                    val file = File(context.cacheDir, "picture-${UuidV7.generate()}")
                                    try {
                                        val picture = readPickedFile(context, uri).second
                                        withContext(Dispatchers.IO) { file.writeBytes(picture) }
                                        model.terminals.act(
                                            "session.picture",
                                            target,
                                            fields("path" to file.absolutePath)
                                        )
                                    } finally {
                                        withContext(NonCancellable + Dispatchers.IO) { file.delete() }
                                    }
                                }
                            }
                        }
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

                    !session.isConnected -> SessionState(session, online, connection, model.account::retryConnection)

                    session.unreachable(online) && state !is TerminalConnection.State.Closed -> Unit

                    else -> RenderState(state, connection)
                }
                clipboardAnswer?.let { answer ->
                    ConfirmDialog(
                        session?.name.orEmpty(),
                        stringResource(
                            R.string.clipboard_request
                        ),
                        stringResource(R.string.approve),
                        {
                            answer("")
                            clipboardAnswer = null
                        },
                        {
                            val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                            val text = clipboard.primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
                            answer(text)
                            clipboardAnswer = null
                        }
                    )
                }
            }
        }
        if (composing) {
            MessageComposer(
                draft,
                false,
                change = { draft = it },
                send = {
                    if (view?.paste(draft, enter = true) == true) {
                        draft = ""
                        composing = false
                        focus.clearFocus()
                        keyboard?.hide()
                    }
                },
                enabled = session?.canControl == true && state == TerminalConnection.State.Rendered,
                focusRequester = composerFocus
            )
            Row(Modifier.fillMaxWidth().navigationBarsPadding(), horizontalArrangement = Arrangement.End) {
                ActionIcon(AppSymbol.KEYBOARD, R.string.keyboard, {
                    composing = false
                    view?.showKeyboard()
                })
                ActionIcon(AppSymbol.CLOSE, R.string.close, {
                    composing = false
                    keyboard?.hide()
                })
            }
        } else if (typing) {
            KeyRow(view, control) {
                if (landscape) {
                    ActionIcon(AppSymbol.COMPOSE, R.string.compose, { composing = true })
                    ActionIcon(AppSymbol.HIDE_KEYBOARD, R.string.hide_keyboard, { keyboard?.hide() })
                    TerminalActions(model, session, view, back, canvas, {
                        renaming = true
                    }, { closing = true })
                }
            }
        } else {
            Row(
                Modifier.fillMaxWidth().navigationBarsPadding().padding(10.dp),
                horizontalArrangement = Arrangement.End
            ) {
                if (session?.needsUser == true && session.agent.isAgent) {
                    listOf("1", "2", "3").forEach { answer -> KeyCap(answer) { view?.paste(answer, enter = true) } }
                    Spacer(Modifier.weight(1f))
                }
                ActionIcon(
                    AppSymbol.COMPOSE,
                    R.string.compose,
                    { composing = true },
                    enabled =
                        session?.canControl == true
                )
                ActionIcon(
                    AppSymbol.KEYBOARD,
                    R.string.keyboard,
                    { view?.showKeyboard() },
                    enabled = state == TerminalConnection.State.Rendered
                )
            }
        }
    }
    if (renaming && session?.isOwner == true) {
        TextEntryDialog(
            stringResource(R.string.rename),
            stringResource(R.string.name),
            session.name,
            dismiss = { renaming = false }
        ) { model.terminals.act("session.rename", session, fields("name" to it.trim())) }
    }
    if (closing && session != null) {
        ConfirmDialog(
            stringResource(R.string.close_terminal),
            stringResource(R.string.close_terminal_message),
            stringResource(R.string.close_terminal),
            { closing = false },
            {
                scope.launch {
                    model.runAction {
                        model.terminals.act("session.close", session)
                        closing = false
                        back()
                    }
                }
            },
            destructive = true
        )
    }
}

@Composable
private fun CanvasNotice(session: RuntimeSession, key: String, open: () -> Unit, dismiss: () -> Unit) {
    val colors = KodosiTheme.colors
    val item = session.canvas?.items?.firstOrNull { it.key == key }
    val browser = session.canvas?.browser?.tabs?.firstOrNull { it.active }
    val title = item?.name ?: browser?.title?.takeIf { it.isNotBlank() } ?: stringResource(R.string.browser)
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 6.dp)
            .raised(colors, RoundedCornerShape(Radius.lg))
            .semantics { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically
    ) {
        ListButton(open, Modifier.weight(1f)) {
            ListLine(title, subtitle = stringResource(R.string.canvas), leading = {
                if (item != null) {
                    FileIcon(item.name, item.kind, 26.dp)
                } else {
                    AppIcon(AppSymbol.BROWSER, Modifier.size(24.dp), tint = colors.accent)
                }
            })
        }
        ActionIcon(AppSymbol.CLOSE, R.string.close, dismiss)
    }
}

@Composable
private fun TerminalActions(
    model: AppModel,
    session: RuntimeSession?,
    view: TerminalView?,
    back: () -> Unit,
    canvas: (String?) -> Unit,
    rename: () -> Unit,
    close: () -> Unit
) {
    val scope = rememberCoroutineScope()
    var menu by remember { mutableStateOf(false) }
    Box {
        ActionIcon(AppSymbol.SETTINGS, R.string.terminal_actions, { menu = true }, open = menu)
        KodosiMenu(menu, { menu = false }) {
            if (session?.canControl == true && session.programLive != null) {
                AgentSettingsSection(model, session)
            }
            if (session?.canControl == true || session?.canvas?.browser != null) {
                KodosiMenuItem(text = { Text(stringResource(R.string.wake_browser)) }, onClick = {
                    menu = false
                    canvas("browser")
                })
            }
            if (session?.isOwner == true) {
                KodosiMenuItem(text = { Text(stringResource(R.string.rename)) }, onClick = {
                    menu = false
                    rename()
                })
                KodosiMenuItem(text = {
                    Text(stringResource(R.string.interrupt))
                }, enabled = session.canControl, onClick = {
                    menu = false
                    scope.launch { model.runAction { model.terminals.act("session.interrupt", session) } }
                })
                KodosiMenuItem(text = { Text(stringResource(R.string.close_terminal)) }, onClick = {
                    menu = false
                    close()
                })
                if (session.hostOffline) {
                    KodosiMenuItem(text = { Text(stringResource(R.string.hide_terminal)) }, onClick = {
                        menu = false
                        scope.launch {
                            model.runAction {
                                model.terminals.act("session.hide", session)
                                back()
                            }
                        }
                    })
                }
            } else if (session != null) {
                KodosiMenuItem(text = { Text(stringResource(R.string.leave_terminal)) }, onClick = {
                    menu = false
                    scope.launch {
                        model.runAction {
                            model.terminals.act("session.leave", session)
                            back()
                        }
                    }
                })
            }
            KodosiMenuItem(text = {
                Text(stringResource(R.string.paste))
            }, enabled = session?.canControl == true, onClick = {
                menu = false
                view?.pasteClipboard()
            })
            KodosiMenuItem(text = { Text(stringResource(R.string.select_text)) }, onClick = {
                menu = false
                view?.selectAll()
            })
        }
    }
}

@Composable
private fun Header(
    session: RuntimeSession?,
    online: Boolean,
    retryConnection: () -> Unit,
    back: () -> Unit,
    rename: () -> Unit
) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 6.dp)
    ) {
        ActionIcon(AppSymbol.BACK, R.string.back, back)
        if (session != null) {
            AgentMark(
                session.agent,
                28.dp,
                account = session.programAccount,
                activity = if (session.isWorking) MarkActivity.WORKING else MarkActivity.AWAKE
            )
            Spacer(Modifier.width(10.dp))
            Column(
                Modifier.weight(1f).heightIn(min = 48.dp).clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    enabled = session.isOwner,
                    role = Role.Button,
                    onClickLabel = stringResource(R.string.rename),
                    onClick = rename
                ),
                verticalArrangement = Arrangement.Center
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        session.name,
                        style = KodosiType.headline,
                        color = colors.ink,
                        modifier = Modifier.weight(1f, fill = false),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    session.sign()?.let {
                        Spacer(Modifier.width(6.dp))
                        StatusSign(it, 13.dp)
                    }
                }
                if (session.unreachable(online)) {
                    TerminalCaption(session, online)
                } else {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        AppIcon(AppSymbol.COMPUTER, Modifier.size(10.dp), tint = colors.inkFaint)
                        Text(
                            session.hostName ?: stringResource(R.string.remote_computer),
                            style = KodosiType.caption,
                            color = colors.inkFaint,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false)
                        )
                        session.folderName?.let {
                            AppIcon(AppSymbol.FORWARD, Modifier.size(7.dp), tint = colors.inkFaint)
                            AppIcon(AppSymbol.FOLDER, Modifier.size(10.dp))
                            Text(
                                it,
                                style = KodosiType.caption,
                                color = colors.inkMuted,
                                maxLines = 1,
                                overflow = TextOverflow.MiddleEllipsis,
                                modifier = Modifier.weight(1f, fill = false)
                            )
                        }
                    }
                }
            }
            if (session.isConnected && session.unreachable(online)) {
                KodosiButton(stringResource(R.string.try_again), onClick = retryConnection, kind = ButtonKind.GHOST)
            }
        }
    }
}

@Composable
private fun SessionState(
    session: RuntimeSession,
    online: Boolean,
    connection: TerminalConnection,
    retryConnection: () -> Unit
) {
    when {
        session.connectionState == RuntimeSession.Connection.BLOCKED -> StateCard(
            statusLabel(session),
            session.message,
            working = false
        )

        session.unreachable(online) -> StateCard(
            stringResource(R.string.offline),
            stringResource(R.string.terminal_opens_when_connection_back),
            working = false
        ) { KodosiButton(stringResource(R.string.try_again), onClick = retryConnection) }

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
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun KeyRow(view: TerminalView?, control: Boolean, actions: @Composable () -> Unit) {
    FlowRow(
        horizontalArrangement = Arrangement.SpaceBetween,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 4.dp)
    ) {
        KeyCap("esc", stringResource(R.string.key_escape)) { view?.press(TerminalKernel.Key.ESCAPE) }
        KeyCap("tab", stringResource(R.string.key_tab)) { view?.press(TerminalKernel.Key.TAB) }
        KeyCap("ctrl", stringResource(R.string.key_control), on = control) { view?.let { it.control = !it.control } }
        Spacer(Modifier.width(2.dp))
        KeyCap("", stringResource(R.string.key_left), symbol = AppSymbol.ARROW_LEFT) {
            view?.press(TerminalKernel.Key.LEFT)
        }
        KeyCap("", stringResource(R.string.key_up), symbol = AppSymbol.ARROW_UP) { view?.press(TerminalKernel.Key.UP) }
        KeyCap("", stringResource(R.string.key_down), symbol = AppSymbol.ARROW_DOWN) {
            view?.press(TerminalKernel.Key.DOWN)
        }
        KeyCap("", stringResource(R.string.key_right), symbol = AppSymbol.ARROW_RIGHT) {
            view?.press(TerminalKernel.Key.RIGHT)
        }
        Spacer(Modifier.width(2.dp))
        actions()
    }
}

@Composable
private fun KeyCap(
    cap: String,
    label: String = cap,
    on: Boolean = false,
    enabled: Boolean = true,
    symbol: AppSymbol? = null,
    press: () -> Unit
) {
    val colors = KodosiTheme.colors
    val shape = RoundedCornerShape(Radius.sm)
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    val fill = if (on) colors.accentSoft else colors.lifted
    val ink = when {
        !enabled -> colors.inkFaint
        on -> colors.accentStrong
        else -> colors.inkMuted
    }
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .defaultMinSize(minWidth = 48.dp, minHeight = 48.dp)
            .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = press)
            .semantics {
                contentDescription = label
                if (on) selected = true
            }
            .padding(horizontal = 4.dp, vertical = 3.dp)
    ) {
        Box(
            Modifier.defaultMinSize(minWidth = 32.dp, minHeight = 42.dp)
                .bezel(colors, shape, if (pressed) lerp(fill, Color.Black, 0.08f) else fill, quiet = true)
                .border(1.5.dp, if (focused) colors.accentStrong else Color.Transparent, shape)
                .padding(horizontal = 8.dp),
            contentAlignment = Alignment.Center
        ) {
            if (symbol == null) {
                Text(
                    cap,
                    style = KodosiType.footnote.copy(fontFamily = FontFamily.Monospace, fontSize = 12.5.sp),
                    color = ink,
                    maxLines = 1,
                    softWrap = false
                )
            } else {
                AppIcon(symbol, Modifier.size(12.dp), tint = ink)
            }
        }
    }
}
