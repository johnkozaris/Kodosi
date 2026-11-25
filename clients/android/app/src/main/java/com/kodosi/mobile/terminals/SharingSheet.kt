package com.kodosi.mobile.terminals

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ModalBottomSheetProperties
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.people.Person
import com.kodosi.mobile.projects.Project
import com.kodosi.mobile.runtime.Refused
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.CheckRow
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.IconTile
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListButton
import com.kodosi.mobile.ui.ListGroup
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.OfflineNote
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.ProjectMark
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.TileTint
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.snappy
import com.kodosi.mobile.ui.well
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** The project the owner picked, or none; until they pick, the sheet shows the terminal's own. */
private data class ProjectPick(val projectId: String?)

/** The people the owner picked, and those the terminal had when they began, which the runtime checks are unchanged. */
private data class PeoplePick(val had: Set<String>, val chosen: Set<String>)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SharingSheet(model: AppModel, session: RuntimeSession, dismiss: () -> Unit, addFriend: () -> Unit) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val sheet = rememberBottomSheetState(
        initialValue = SheetValue.Hidden,
        enabledValues = setOf(SheetValue.Hidden, SheetValue.Expanded)
    )
    var saving by remember { mutableStateOf(false) }
    val close: () -> Unit = { scope.launch { sheet.hide() }.invokeOnCompletion { dismiss() } }
    ModalBottomSheet(
        onDismissRequest = { if (!saving) dismiss() },
        sheetState = sheet,
        sheetMaxWidth = 600.dp,
        sheetGesturesEnabled = !saving,
        properties = ModalBottomSheetProperties(
            shouldDismissOnBackPress = !saving,
            shouldDismissOnClickOutside = !saving
        ),
        shape = RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet),
        containerColor = colors.surface,
        tonalElevation = 0.dp
    ) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(start = 20.dp, end = 20.dp, bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                AgentMark(
                    session.agent,
                    34.dp,
                    account = session.programAccount,
                    activity = if (session.isWorking) MarkActivity.WORKING else MarkActivity.AWAKE
                )
                Column(Modifier.weight(1f).padding(start = 11.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        if (session.isOwner) stringResource(R.string.share_terminal, session.name) else session.name,
                        style = KodosiType.headline,
                        color = colors.ink,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.semantics { heading() }
                    )
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        AppIcon(AppSymbol.LOCK, Modifier.size(10.dp), tint = colors.inkMuted)
                        Text(
                            stringResource(R.string.encrypted_end_to_end),
                            style = KodosiType.caption,
                            color = colors.inkMuted
                        )
                    }
                }
                ActionIcon(AppSymbol.CLOSE, R.string.close, close, enabled = !saving)
            }
            if (session.isOwner) {
                OwnerChoices(model, session, saving, { saving = it }, close, addFriend)
            } else {
                SharedBy(session)
            }
        }
    }
}

@Composable
private fun OwnerChoices(
    model: AppModel,
    live: RuntimeSession,
    saving: Boolean,
    setSaving: (Boolean) -> Unit,
    close: () -> Unit,
    addFriend: () -> Unit
) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val projects by model.projects.state.collectAsStateWithLifecycle()
    val people by model.people.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    var projectPick by remember { mutableStateOf<ProjectPick?>(null) }
    var peoplePick by remember { mutableStateOf<PeoplePick?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    // While a change is on its way, the sheet keeps showing what it sent, so updates do not flash by.
    var sent by remember { mutableStateOf<RuntimeSession?>(null) }
    val session = sent ?: live
    val projectId = projectPick.let { pick -> if (pick == null) session.projectId else pick.projectId }
    val shared = session.sharedWith.toSet()
    val had = peoplePick?.had ?: shared
    val chosen = peoplePick?.chosen ?: shared
    val projectChanged = projectId != session.projectId
    val changed = projectChanged || chosen != had
    Unfolding(projects.projects.isNotEmpty()) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel(stringResource(R.string.a_project))
            FlowRow(
                Modifier.selectableGroup(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                projects.projects.forEach { project ->
                    val selected = projectId == project.id
                    ProjectChoice(project, selected, enabled = !saving) {
                        projectPick = ProjectPick(if (selected) null else project.id)
                        if (!selected) peoplePick = null
                        failure = null
                    }
                }
            }
        }
    }
    Unfolding(projectId == null && session.projectId == null) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel(stringResource(R.string.friends))
            when {
                !people.loaded -> ProgressCaption(stringResource(R.string.loading))

                people.friends.isEmpty() -> ListGroup {
                    ListButton(onClick = addFriend, enabled = !saving) {
                        ListLine(
                            stringResource(R.string.add_friend),
                            leading = { IconTile(AppSymbol.ADD, TileTint.blue) }
                        )
                    }
                }

                else -> Column {
                    people.friends.forEach { friend ->
                        val selected = friend.userId in chosen
                        val changedIdentity = friend.identityState == "changed"
                        // A changed identity may be someone else's, so it gets no new access until it is trusted.
                        val blocked = changedIdentity && !selected
                        CheckRow(
                            friend.name,
                            selected,
                            { on ->
                                val pick = peoplePick ?: PeoplePick(shared, shared)
                                peoplePick = pick.copy(
                                    chosen = if (on) pick.chosen + friend.userId else pick.chosen - friend.userId
                                )
                                failure = null
                            },
                            modifier = Modifier.alpha(if (blocked) 0.5f else 1f),
                            enabled = !saving && !blocked,
                            subtitle = if (changedIdentity) {
                                stringResource(R.string.identity_changed)
                            } else {
                                "@${friend.handle}"
                            },
                            leading = { Avatar(friend.name, friend.userId) }
                        )
                    }
                }
            }
        }
    }
    Unfolding(failure) { ErrorNote(it, Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) }
    Unfolding(!account.online) { OfflineNote(model.account::retryConnection, Modifier.fillMaxWidth()) }
    KodosiButton(
        if (saving) {
            stringResource(R.string.working)
        } else {
            saveTitle(projectId, projectChanged, changed, had, chosen, projects.projects, people.friends)
        },
        onClick = {
            sent = live
            setSaving(true)
            failure = null
            scope.launch {
                try {
                    if (projectChanged) {
                        model.terminals.act("session.attachProject", live, fields("projectId" to projectId))
                    } else {
                        model.terminals.act(
                            "session.share",
                            live,
                            fields("userIds" to chosen.sorted(), "expectedUserIds" to had.sorted())
                        )
                    }
                    close()
                    return@launch
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (_: Refused) {
                } catch (error: Exception) {
                    failure = error.localizedMessage ?: model.words.didNotWork
                }
                sent = null
                setSaving(false)
            }
        },
        enabled = changed && account.online && !saving,
        size = ButtonSize.LARGE,
        modifier = Modifier.fillMaxWidth()
    )
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.share_caption), style = KodosiType.caption, color = colors.inkFaint)
        Unfolding(projectId != null) {
            Text(stringResource(R.string.share_project_caption), style = KodosiType.caption, color = colors.inkFaint)
        }
    }
}

@Composable
private fun saveTitle(
    projectId: String?,
    projectChanged: Boolean,
    changed: Boolean,
    had: Set<String>,
    chosen: Set<String>,
    projects: List<Project>,
    friends: List<Person>
): String {
    val single = chosen.singleOrNull()?.let { id -> friends.firstOrNull { it.userId == id }?.name }
    return when {
        projectChanged && projectId == null -> stringResource(R.string.stop_sharing)

        projectChanged -> projects.firstOrNull { it.id == projectId }?.name?.let {
            stringResource(R.string.share_with_project, it)
        } ?: stringResource(R.string.share_action)

        !changed && (projectId != null || chosen.isNotEmpty()) -> stringResource(R.string.shared)

        chosen.isEmpty() -> stringResource(if (had.isEmpty()) R.string.share_action else R.string.stop_sharing)

        single != null -> stringResource(R.string.share_with, single)

        else -> pluralStringResource(R.plurals.share_with_people, chosen.size, chosen.size)
    }
}

@Composable
private fun SharedBy(session: RuntimeSession) {
    val owner = session.ownerName ?: session.hostName ?: stringResource(R.string.remote_computer)
    ListGroup {
        ListLine(
            session.projectName?.let { stringResource(R.string.shared_by_in, owner, it) }
                ?: stringResource(R.string.shared_by, owner),
            leading = { Avatar(owner, session.ownerUserId ?: owner) }
        )
    }
}

// The name beside the avatar already says who it is, so its initials stay silent.
@Composable
private fun Avatar(name: String, id: String) {
    Box(Modifier.clearAndSetSemantics {}) { PersonAvatar(name, id, 32.dp) }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text,
        style = KodosiType.subhead,
        color = KodosiTheme.colors.inkMuted,
        modifier = Modifier.padding(start = 4.dp).semantics { heading() }
    )
}

@Composable
private fun ProjectChoice(project: Project, selected: Boolean, enabled: Boolean, choose: () -> Unit) {
    val colors = KodosiTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val focused by interaction.collectIsFocusedAsState()
    val scale by animateFloatAsState(if (pressed) 0.97f else 1f, snappy(), label = "project press")
    Row(
        Modifier
            .selectable(
                selected,
                interaction,
                indication = null,
                enabled = enabled,
                role = Role.RadioButton,
                onClick = choose
            )
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
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
        ProjectMark(project.id, 22.dp)
        Text(
            project.name,
            style = KodosiType.subhead,
            color = if (selected) colors.ink else colors.inkMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false)
        )
        if (selected) AppIcon(AppSymbol.CHECK, Modifier.size(10.dp), tint = colors.accentStrong)
    }
}
