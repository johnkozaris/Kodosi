package com.kodosi.mobile.projects

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.TerminalCard
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.DockItem
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiSendKey
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MessageComposer
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.ProjectMark
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.SectionDock
import com.kodosi.mobile.ui.SelectableWords
import com.kodosi.mobile.ui.raised
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
fun ProjectScreen(
    model: AppModel,
    projectId: String,
    back: () -> Unit,
    terminal: (String) -> Unit,
    start: (String) -> Unit
) {
    val projects by model.projects.state.collectAsStateWithLifecycle()
    val project = model.projects.project(projectId)
    val snapshot = projects.snapshots[projectId]
    val account by model.account.state.collectAsStateWithLifecycle()
    val members = projects.details[projectId]?.members
    val scope = rememberCoroutineScope()
    var section by rememberSaveable(projectId) { mutableIntStateOf(0) }
    var wideSection by rememberSaveable(projectId) { mutableIntStateOf(2) }
    var beforeInfo by rememberSaveable(projectId) { mutableIntStateOf(0) }
    var failure by remember(projectId) { mutableStateOf<String?>(null) }
    var loading by remember(projectId) { mutableStateOf(true) }
    LaunchedEffect(projectId) {
        launch { model.runAction { model.projects.open(projectId) } }
        try {
            model.projects.read(projectId)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            failure = error.localizedMessage
        } finally {
            loading = false
        }
    }
    BackHandler(section == 4) { section = beforeInfo }
    Column(Modifier.fillMaxSize().imePadding()) {
        PageHeader(project?.name ?: stringResource(R.string.projects), {
            if (section == 4) section = beforeInfo else back()
        }, titleContent = {
            Row(horizontalArrangement = Arrangement.spacedBy(9.dp), verticalAlignment = Alignment.CenterVertically) {
                ProjectMark(projectId, 28.dp)
                Column {
                    Text(
                        project?.name ?: stringResource(R.string.projects),
                        style = KodosiType.headline,
                        color = KodosiTheme.colors.ink,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                    if (members != null) {
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            AppIcon(AppSymbol.LOCK, Modifier.size(9.dp), tint = KodosiTheme.colors.inkFaint)
                            Text(
                                pluralStringResource(R.plurals.member_count, members.size, members.size),
                                style = KodosiType.caption,
                                color = KodosiTheme.colors.inkFaint
                            )
                        }
                    }
                }
            }
        }, actions = {
            if (section != 4) {
                Row(horizontalArrangement = Arrangement.spacedBy((-6).dp)) {
                    members.orEmpty().take(3).forEach {
                        PersonAvatar(it.name, it.userId, 22.dp, own = it.userId == account.userId)
                    }
                }
                ActionIcon(AppSymbol.INFO, R.string.project_info, {
                    beforeInfo = section
                    section = 4
                })
            }
        })
        if (project == null && projects.loaded) {
            EmptyState(stringResource(R.string.project_unavailable))
            return@Column
        }
        val sections = listOf(
            DockItem(stringResource(R.string.conversation), AppSymbol.CHAT),
            DockItem(stringResource(R.string.tasks), AppSymbol.TASKS, snapshot?.tasks?.count { !it.closed } ?: 0),
            DockItem(stringResource(R.string.terminals), AppSymbol.TERMINAL),
            DockItem(stringResource(R.string.repositories), AppSymbol.REPOSITORY)
        )
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            val wide = maxWidth >= 720.dp && section != 4
            val chatWidth = (maxWidth * 0.42f).coerceIn(340.dp, 480.dp)
            if (wide) {
                Row(
                    Modifier.fillMaxSize().padding(end = 12.dp, bottom = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Column(Modifier.weight(1f)) {
                        SectionDock(sections.drop(1), wideSection - 1) { wideSection = it + 1 }
                        ProjectWork(model, projectId, snapshot, wideSection, terminal, start)
                    }
                    val shape = RoundedCornerShape(Radius.xl)
                    Column(
                        Modifier.width(chatWidth).fillMaxHeight()
                            .raised(KodosiTheme.colors, shape, fill = KodosiTheme.colors.sheet)
                            .clip(shape)
                    ) {
                        Conversation(model, projectId, snapshot, failure, loading, terminal)
                    }
                }
            } else {
                Column(Modifier.fillMaxSize()) {
                    if (section != 4) {
                        SectionDock(sections, section) {
                            section = it
                            if (it in 1..3) wideSection = it
                        }
                    }
                    Box(Modifier.weight(1f)) {
                        when (section) {
                            0 -> Conversation(model, projectId, snapshot, failure, loading, terminal)
                            4 -> ProjectInfo(model, projectId, back)
                            else -> ProjectWork(model, projectId, snapshot, section, terminal, start)
                        }
                    }
                }
            }
        }
        if (failure != null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ErrorNote(failure.orEmpty(), Modifier.weight(1f))
                KodosiTextButton(onClick = {
                    loading = true
                    scope.launch {
                        try {
                            model.projects.read(projectId)
                            failure = null
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            failure = error.localizedMessage
                        } finally {
                            loading = false
                        }
                    }
                }) { Text(stringResource(R.string.try_again)) }
            }
        }
    }
}

@Composable
private fun Conversation(
    model: AppModel,
    projectId: String,
    snapshot: ProjectSnapshot?,
    failure: String?,
    loading: Boolean,
    terminal: (String) -> Unit
) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val account by model.account.state.collectAsStateWithLifecycle()
    var draft by rememberSaveable(projectId) { mutableStateOf(model.projects.drafts[projectId].orEmpty()) }
    var sending by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var earlier by remember { mutableStateOf(false) }
    val scroll = rememberLazyListState()
    val messages = snapshot?.messages.orEmpty()
    val following by remember { derivedStateOf { !scroll.canScrollBackward } }
    LifecycleResumeEffect(projectId, following) {
        if (following) {
            model.projects.reading = projectId
            model.projects.markRead(projectId)
        }
        onPauseOrDispose { if (model.projects.reading == projectId) model.projects.reading = null }
    }
    LaunchedEffect(messages.lastOrNull()?.id, following) {
        if (following && model.projects.reading == projectId) model.projects.markRead(projectId)
    }
    Column(Modifier.fillMaxSize().background(colors.sheet)) {
        LazyColumn(
            state = scroll,
            reverseLayout = true,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp)
        ) {
            items(messages.asReversed(), key = { it.id }) { message ->
                MessageRow(model, projectId, message, message.isOwn(account.userId), terminal)
            }
            item {
                if (snapshot?.hasOlder == true) {
                    KodosiTextButton(enabled = !earlier, onClick = {
                        earlier = true
                        scope.launch {
                            model.runAction { model.projects.read(projectId, messages.firstOrNull()?.sequence) }
                            earlier = false
                        }
                    }) { Text(stringResource(R.string.load_older)) }
                }
                if (snapshot?.historyTrimmed ==
                    true
                ) {
                    Text(
                        stringResource(R.string.history_trimmed),
                        style = KodosiType.footnote,
                        color = colors.inkFaint
                    )
                }
            }
            if (loading &&
                snapshot == null
            ) {
                item { ProgressCaption(stringResource(R.string.loading), Modifier.padding(24.dp)) }
            }
            if (!loading && failure == null &&
                messages.isEmpty()
            ) {
                item { EmptyState(stringResource(R.string.no_messages)) }
            }
        }
        error?.let { ErrorNote(it, Modifier.padding(horizontal = 12.dp)) }
        MessageComposer(
            draft,
            sending,
            change = {
                draft = it
                model.projects.drafts[projectId] = it
            },
            send = {
                val sent = draft
                sending = true
                error = null
                scope.launch {
                    try {
                        model.projects.post(projectId, sent)
                        if (draft == sent) draft = ""
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (failure: Exception) {
                        error = failure.localizedMessage
                    } finally {
                        sending = false
                    }
                }
            }
        )
    }
}

@Composable
private fun MessageRow(
    model: AppModel,
    projectId: String,
    message: ProjectMessage,
    own: Boolean,
    terminal: (String) -> Unit
) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    var actions by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
        if (own) {
            Spacer(Modifier.width(48.dp))
        } else {
            if (message.agent != null) {
                AgentMark(AgentKind.named(message.agent), 24.dp)
            } else {
                PersonAvatar(message.authorName, message.authorId, 24.dp)
            }
            Spacer(Modifier.width(8.dp))
        }
        Column(
            Modifier.weight(1f),
            horizontalAlignment = if (own) Alignment.End else Alignment.Start,
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            if (!own) {
                Text(
                    message.agent?.let { "$it \u00b7 ${message.authorName}" } ?: message.authorName,
                    style = KodosiType.caption,
                    color = colors.inkMuted
                )
            }
            val shape = RoundedCornerShape(
                topStart = 14.dp,
                topEnd = 14.dp,
                bottomStart = if (own) 14.dp else 4.dp,
                bottomEnd = if (own) 4.dp else 14.dp
            )
            SelectableWords(
                message.text,
                Modifier.widthIn(max = 640.dp).background(if (own) colors.accentSoft else colors.raised, shape)
                    .padding(horizontal = 12.dp, vertical = 8.dp)
            )
            Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(projectTime(message.createdAt), style = KodosiType.caption, color = colors.inkFaint)
                Box {
                    ActionIcon(AppSymbol.MORE, R.string.more, { actions = true }, open = actions, face = 22.dp)
                    KodosiMenu(actions, { actions = false }) {
                        KodosiMenuItem(text = { Text(stringResource(R.string.create_task)) }, onClick = {
                            actions = false
                            scope.launch {
                                model.runAction {
                                    model.projects.createTask(
                                        projectId,
                                        message.text.lineSequence().first().take(120),
                                        message.text,
                                        emptySet()
                                    )
                                }
                            }
                        })
                        message.terminalId?.let { id ->
                            KodosiMenuItem(text = { Text(stringResource(R.string.terminals)) }, onClick = {
                                actions = false
                                terminal(id)
                            })
                        }
                    }
                }
            }
        }
        if (!own) Spacer(Modifier.width(12.dp))
    }
}

@Composable
private fun ProjectWork(
    model: AppModel,
    projectId: String,
    snapshot: ProjectSnapshot?,
    section: Int,
    terminal: (String) -> Unit,
    start: (String) -> Unit
) {
    when (section) {
        1 -> ProjectTasks(model, projectId, snapshot)
        2 -> ProjectTerminals(model, projectId, terminal, start)
        3 -> ProjectRepositories(model, projectId, snapshot)
    }
}

@Composable
private fun ProjectTerminals(model: AppModel, projectId: String, terminal: (String) -> Unit, start: (String) -> Unit) {
    val terminals by model.terminals.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val sessions = terminals.sessions.filter { it.projectId == projectId }
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        item { KodosiTextButton(onClick = { start(projectId) }) { Text(stringResource(R.string.new_terminal)) } }
        items(sessions, key = { it.id }) { session ->
            TerminalCard(session, account.online, { terminal(session.id) }, Modifier.padding(vertical = 5.dp))
        }
        if (sessions.isEmpty()) {
            item {
                EmptyState(stringResource(R.string.terminals_empty_title), symbol = AppSymbol.TERMINAL)
            }
        }
    }
}
