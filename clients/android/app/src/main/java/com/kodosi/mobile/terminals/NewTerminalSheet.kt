package com.kodosi.mobile.terminals

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ModalBottomSheetProperties
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.IconTile
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListButton
import com.kodosi.mobile.ui.ListGroup
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MenuPicker
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.TileTint
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.terminalTile
import com.kodosi.mobile.ui.tint
import com.kodosi.mobile.ui.well
import java.text.NumberFormat
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

data class NewTerminalRequest(val computer: String? = null, val folder: String? = null, val room: String? = null)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewTerminalSheet(model: AppModel, request: NewTerminalRequest, dismiss: () -> Unit, started: (String) -> Unit) {
    val colors = KodosiTheme.colors
    val account by model.account.state.collectAsStateWithLifecycle()
    var computer by rememberSaveable {
        mutableStateOf(
            request.computer ?: model.preferences.launchComputer ?: account.computers.firstOrNull()?.deviceId
        )
    }
    val chosen = account.computers.firstOrNull { it.deviceId == computer } ?: account.computers.firstOrNull()
    val launcher = remember(chosen?.deviceId) { chosen?.let { Launcher(model.commands, it.deviceId) } }
    val state = launcher?.state?.collectAsStateWithLifecycle()?.value
    var prompt by rememberSaveable { mutableStateOf("") }
    var folder by rememberSaveable(chosen?.deviceId) {
        mutableStateOf(request.folder ?: chosen?.deviceId?.let(model.preferences::launchFolder))
    }
    var browsing by remember(chosen?.deviceId) { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    var attempt by remember { mutableIntStateOf(0) }
    val scope = rememberCoroutineScope()
    val unnamed = stringResource(R.string.new_terminal)
    LaunchedEffect(launcher, attempt, account.online) {
        if (launcher == null || !account.online) return@LaunchedEffect
        failure = null
        try {
            launcher.refresh()
            launcher.folders()
            if (folder == null) folder = launcher.state.value.folders?.recent?.firstOrNull()
            launcher.usage()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            failure = error.localizedMessage ?: model.words.didNotWork
        }
    }
    ModalBottomSheet(
        onDismissRequest = { if (!busy) dismiss() },
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        sheetMaxWidth = 600.dp,
        sheetGesturesEnabled = !busy,
        properties = ModalBottomSheetProperties(shouldDismissOnBackPress = !busy, shouldDismissOnClickOutside = !busy),
        shape = RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet),
        containerColor = colors.surface,
        tonalElevation = 0.dp
    ) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.94f).imePadding()) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                if (browsing) ActionIcon(AppSymbol.BACK, R.string.back, { browsing = false })
                Text(
                    stringResource(if (browsing) R.string.folder else R.string.new_terminal),
                    Modifier.weight(1f),
                    style = KodosiType.title,
                    color = colors.ink
                )
                KodosiTextButton(onClick = dismiss, enabled = !busy) { Text(stringResource(R.string.cancel)) }
            }
            if (!account.online) {
                EmptyState(
                    stringResource(R.string.offline),
                    stringResource(R.string.terminal_opens_when_connection_back),
                    AppSymbol.COMPUTER
                ) {
                    KodosiButton(stringResource(R.string.try_again), onClick = model.account::retryConnection)
                }
            } else if (chosen == null || launcher == null || state == null) {
                EmptyState(
                    stringResource(R.string.no_computer),
                    stringResource(R.string.no_computer_message),
                    AppSymbol.COMPUTER
                )
            } else if (browsing) {
                FolderChoices(launcher, model, folder, Modifier.weight(1f)) {
                    folder = it
                    browsing = false
                }
            } else {
                val agent = state.agents.firstOrNull { it.agent == state.agent }
                LazyColumn(
                    Modifier.weight(1f).fillMaxWidth().padding(horizontal = 20.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp)
                ) {
                    item {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            ProgramChoice(AgentKind.SHELL, state.agent == null, enabled = state.loaded && !busy) {
                                scope.launch { model.runAction { launcher.chooseAgent(null) } }
                            }
                            state.agents.forEach { entry ->
                                ProgramChoice(
                                    AgentKind.of(entry.agent),
                                    entry.agent == state.agent,
                                    enabled = !busy
                                ) {
                                    scope.launch { model.runAction { launcher.chooseAgent(entry.agent) } }
                                }
                            }
                        }
                    }
                    if ((!state.loaded || (agent != null && !state.modelsLoaded)) && failure == null) {
                        item { ProgressCaption(stringResource(R.string.loading)) }
                    }
                    item {
                        ListGroup {
                            MenuPicker(
                                stringResource(R.string.computer),
                                chosen.label,
                                account.computers.map { it.deviceId to it.label },
                                leading = { IconTile(AppSymbol.COMPUTER, TileTint.graphite) },
                                enabled = !busy
                            ) { computer = it }
                            ListButton(onClick = { browsing = true }, enabled = !busy) {
                                ListLine(
                                    stringResource(R.string.folder),
                                    subtitle = folder ?: state.folders?.home ?: "~",
                                    leading = { IconTile(AppSymbol.FOLDER, TileTint.teal) }
                                ) {
                                    AppIcon(AppSymbol.FORWARD, Modifier.size(11.dp), tint = colors.inkFaint)
                                }
                            }
                            LauncherOptions(launcher, state, model, !busy)
                        }
                    }
                    if (agent != null) {
                        item {
                            FirstPrompt(agent, prompt, { prompt = it }, !busy)
                        }
                    }
                    failure?.let { error ->
                        item {
                            Column {
                                ErrorNote(error)
                                KodosiTextButton(onClick = { attempt++ }) {
                                    Text(stringResource(R.string.try_again))
                                }
                            }
                        }
                    }
                }
                val startTitle = if (agent == null) {
                    stringResource(R.string.open_terminal)
                } else {
                    stringResource(R.string.start_agent, agentLabel(AgentKind.of(agent.agent)))
                }
                KodosiButton(
                    if (busy) stringResource(R.string.working) else startTitle,
                    onClick = {
                        busy = true
                        failure = null
                        scope.launch {
                            try {
                                launcher.start(unnamed, folder, prompt, request.room)?.let { id ->
                                    model.preferences.launched(launcher.computer, folder)
                                    dismiss()
                                    started(id)
                                }
                            } catch (cancelled: CancellationException) {
                                throw cancelled
                            } catch (error: Exception) {
                                failure = error.localizedMessage
                            } finally {
                                busy = false
                            }
                        }
                    },
                    enabled = !busy && state.loaded && (agent == null || state.modelsLoaded),
                    size = ButtonSize.LARGE,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp)
                )
            }
        }
    }
}

@Composable
private fun ProgramChoice(kind: AgentKind, selected: Boolean, enabled: Boolean, choose: () -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    val scale by animateFloatAsState(
        if (pressed) 0.97f else 1f,
        if (reducedMotion()) snap() else spring(dampingRatio = 0.8f, stiffness = 620f),
        label = "program press"
    )
    Row(
        Modifier.semantics { this.selected = selected }
            .clickable(interaction, indication = null, enabled = enabled, role = Role.RadioButton, onClick = choose)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                alpha = if (enabled) 1f else 0.5f
            }
            .then(
                if (selected) {
                    Modifier.raised(colors, CircleShape, fill = colors.lifted)
                } else {
                    Modifier.well(colors, CircleShape)
                }
            )
            .border(1.5.dp, if (focused) colors.accentStrong else Color.Transparent, CircleShape)
            .defaultMinSize(minHeight = 38.dp).padding(start = 8.dp, end = 13.dp, top = 4.dp, bottom = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        AgentMark(kind, 22.dp)
        Text(agentLabel(kind), style = KodosiType.subhead, color = if (selected) colors.ink else colors.inkMuted)
        if (selected) AppIcon(AppSymbol.CHECK, Modifier.size(10.dp), tint = colors.accentStrong)
    }
}

@Composable
private fun FirstPrompt(agent: AgentInfo, prompt: String, change: (String) -> Unit, enabled: Boolean) {
    KodosiTheme(dark = true) {
        val colors = KodosiTheme.colors
        val focus = LocalFocusManager.current
        val label = stringResource(R.string.first_prompt)
        val stacked = LocalConfiguration.current.fontScale >= 1.5f
        FlowRow(
            Modifier.fillMaxWidth().terminalTile(colors, radius = Radius.lg).padding(14.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp)
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("\u276f", style = KodosiType.mono, color = colors.accent)
                Text(
                    agent.program,
                    style = KodosiType.mono,
                    color = lerp(AgentKind.of(agent.agent).tint(), Color.White, 0.35f)
                )
            }
            BasicTextField(
                prompt, { words ->
                    change(words.replace('\n', ' ').replace('\r', ' '))
                    if ('\n' in words || '\r' in words) focus.clearFocus()
                },
                enabled = enabled, maxLines = 6,
                textStyle = KodosiType.mono.copy(color = colors.ink),
                cursorBrush = SolidColor(colors.accent),
                keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                modifier = (if (stacked) Modifier.fillMaxWidth() else Modifier.weight(1f))
                    .semantics { contentDescription = label },
                decorationBox = { input ->
                    Box {
                        if (prompt.isEmpty()) {
                            Text(
                                stringResource(R.string.type_task),
                                style = KodosiType.mono,
                                color = colors.inkFaint
                            )
                        }
                        input()
                    }
                }
            )
        }
    }
}

@Composable
private fun LauncherOptions(launcher: Launcher, state: LauncherState, model: AppModel, enabled: Boolean) {
    val agent = state.agents.firstOrNull { it.agent == state.agent } ?: return
    val pick = state.pick ?: AgentPick(agent.agent)
    val scope = rememberCoroutineScope()
    if (agent.accounts.size > 1) {
        MenuPicker(
            stringResource(R.string.account),
            agent.accounts.firstOrNull { it.name == pick.account }?.let { it.identity ?: it.name }
                ?: agent.accounts.firstOrNull()?.let { it.identity ?: it.name }.orEmpty(),
            agent.accounts.map { it.name to (it.identity ?: it.name) },
            leading = {
                AgentMark(
                    AgentKind.of(agent.agent),
                    28.dp,
                    account = agent.accounts.indexOfFirst { it.name == pick.account }.coerceAtLeast(0)
                )
            },
            enabled = enabled
        ) {
            scope.launch { model.runAction { launcher.choose(pick.copy(account = it)) } }
        }
    }
    if (state.models.isNotEmpty() || pick.model != null) {
        MenuPicker(
            stringResource(R.string.model),
            state.models.firstOrNull { it.id == pick.model }?.label ?: stringResource(R.string.default_value),
            listOf("" to stringResource(R.string.default_value)) + state.models.map { it.id to it.label },
            leading = { IconTile(AppSymbol.DOSI, AgentKind.of(agent.agent).tint()) },
            enabled = enabled
        ) {
            scope.launch { model.runAction { launcher.choose(pick.copy(model = it.ifBlank { null }, effort = null)) } }
        }
    }
    val efforts = state.models.firstOrNull { it.id == pick.model }?.efforts?.takeIf { it.isNotEmpty() } ?: state.efforts
    if (efforts.isNotEmpty()) {
        MenuPicker(
            stringResource(R.string.effort),
            pick.effort?.let { effortLabel(it) } ?: stringResource(R.string.default_value),
            listOf("" to stringResource(R.string.default_value)) + efforts.map { it to effortLabel(it) },
            leading = { IconTile(AppSymbol.TEXT_SIZE, TileTint.amber) },
            enabled = enabled
        ) {
            scope.launch { model.runAction { launcher.choose(pick.copy(effort = it.ifBlank { null })) } }
        }
    }
    val permissions = pick.permissions?.takeIf(agent.permissions::contains) ?: agent.defaultPermissions
    if (agent.permissions.size > 1) {
        MenuPicker(
            stringResource(R.string.agent_permissions),
            permissions.label(),
            agent.permissions.map { it.name to it.label() },
            leading = { IconTile(AppSymbol.LOCK, TileTint.orange) },
            enabled = enabled,
            details = agent.permissions.associate { it.name to it.detail(AgentKind.of(agent.agent)) }
        ) {
            scope.launch { model.runAction { launcher.choose(pick.copy(permissions = AgentPermissions.valueOf(it))) } }
        }
    }
    val output = pick.output?.takeIf(agent.outputs::contains) ?: agent.defaultOutput
    if (agent.outputs.isNotEmpty()) {
        MenuPicker(
            stringResource(R.string.agent_output),
            output.label(),
            agent.outputs.map { it.name to it.label() },
            leading = { IconTile(AppSymbol.LIST, TileTint.blue) },
            enabled = enabled,
            details = agent.outputs.associate { it.name to it.detail() }
        ) {
            scope.launch { model.runAction { launcher.choose(pick.copy(output = AgentOutput.valueOf(it))) } }
        }
    }
    state.usage.filter { it.string("agent") == agent.agent }.forEach { usage ->
        usage.objects("limits").forEach { limit -> UsageReading(usage, limit) }
    }
}

@Composable
private fun UsageReading(usage: JsonObject, limit: JsonObject) {
    val colors = KodosiTheme.colors
    val used = (limit.number("used") ?: 0).coerceIn(0, 100)
    val tint = when (limit.string("level")) {
        "reached" -> colors.danger
        "near" -> colors.caution
        else -> colors.inkMuted
    }
    val locale = LocalConfiguration.current.locales[0]
    val percent = remember(used, locale) { NumberFormat.getPercentInstance(locale).format(used / 100.0) }
    ListLine(usage.string("account").orEmpty(), subtitle = limit.string("scope"), leading = {
        Canvas(Modifier.size(26.dp)) {
            val stroke = Stroke(2.dp.toPx(), cap = StrokeCap.Round)
            drawArc(colors.ink.copy(alpha = 0.12f), -90f, 360f, false, style = stroke)
            if (used > 0) drawArc(tint, -90f, used * 3.6f, false, style = stroke)
        }
    }) { Text(percent, style = KodosiType.mono, color = tint) }
}

@Composable
private fun FolderChoices(
    launcher: Launcher,
    model: AppModel,
    initial: String?,
    modifier: Modifier,
    choose: (String) -> Unit
) {
    val state by launcher.state.collectAsStateWithLifecycle()
    val folders = state.folders
    val scope = rememberCoroutineScope()
    var loading by remember { mutableStateOf(true) }
    LaunchedEffect(launcher) {
        try {
            model.runAction { launcher.folders(initial) }
        } finally {
            loading = false
        }
    }
    val load: (String) -> Unit = { path ->
        loading = true
        scope.launch {
            try {
                model.runAction { launcher.folders(path) }
            } finally {
                loading = false
            }
        }
    }
    Column(modifier.fillMaxWidth()) {
        LazyColumn(Modifier.weight(1f).padding(horizontal = 20.dp)) {
            item {
                Text(
                    folders?.path.orEmpty(),
                    style = KodosiType.mono,
                    color = KodosiTheme.colors.inkMuted,
                    modifier = Modifier.padding(vertical = 12.dp),
                    overflow = TextOverflow.MiddleEllipsis,
                    maxLines = 2
                )
            }
            if (loading) {
                item { ProgressCaption(stringResource(R.string.loading)) }
            } else {
                if (!folders?.recent.isNullOrEmpty()) {
                    item {
                        MenuPicker(
                            stringResource(R.string.recent_folders),
                            stringResource(R.string.browse_folders),
                            folders.recent.map { it to it },
                            leading = { AppIcon(AppSymbol.FOLDER) },
                            choose = load
                        )
                    }
                }
                folders?.parent?.let { parent ->
                    item {
                        ListButton(onClick = { load(parent) }) {
                            ListLine(stringResource(R.string.parent_folder), leading = { AppIcon(AppSymbol.BACK) })
                        }
                    }
                }
                items(folders?.folders.orEmpty(), key = { it }) { child ->
                    ListButton(onClick = { folders?.let { load(it.child(child)) } }) {
                        ListLine(child, leading = { AppIcon(AppSymbol.FOLDER, tint = TileTint.teal) }) {
                            AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp))
                        }
                    }
                }
            }
        }
        KodosiButton(
            stringResource(R.string.use_folder),
            { folders?.let { choose(it.path) } },
            enabled = !loading && folders != null,
            size = ButtonSize.LARGE,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp)
        )
    }
}
