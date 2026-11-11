package com.kodosi.mobile.rooms

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.sign
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.CheckRow
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiCard
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListGroup
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.RoomMark
import com.kodosi.mobile.ui.SectionTitle
import com.kodosi.mobile.ui.StatusSign
import java.time.Instant
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
fun RoomsScreen(model: AppModel, open: (String) -> Unit) {
    val state by model.rooms.state.collectAsStateWithLifecycle()
    val terminals by model.terminals.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    var creating by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    Column(Modifier.fillMaxSize()) {
        PageHeader(stringResource(R.string.rooms), actions = {
            ActionIcon(AppSymbol.REFRESH, R.string.refresh, model.rooms::refresh)
            ActionIcon(AppSymbol.ADD, R.string.new_room, { creating = true })
        })
        LazyVerticalGrid(
            GridCells.Adaptive(260.dp),
            Modifier.weight(1f).padding(horizontal = 16.dp),
            contentPadding = PaddingValues(bottom = 20.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            if (!state.loaded) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    ProgressCaption(stringResource(R.string.loading), Modifier.padding(32.dp))
                }
            }
            if (state.invitations.isNotEmpty()) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    ListGroup(title = stringResource(R.string.invitations)) {
                        state.invitations.forEach { invitation ->
                            ListLine(invitation.missionName, subtitle = invitation.inviterName, leading = {
                                RoomMark(invitation.missionId)
                            }) {
                                KodosiTextButton(onClick = {
                                    scope.launch { model.runAction { model.rooms.respond(invitation, false) } }
                                }) { Text(stringResource(R.string.decline)) }
                                KodosiTextButton(onClick = {
                                    scope.launch { model.runAction { model.rooms.respond(invitation, true) } }
                                }) { Text(stringResource(R.string.accept)) }
                            }
                        }
                    }
                }
            }
            if (state.loaded && state.rooms.isEmpty()) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    EmptyState(
                        stringResource(R.string.rooms_empty_title),
                        stringResource(R.string.rooms_empty_message)
                    ) {
                        KodosiButton(stringResource(R.string.new_room), { creating = true })
                    }
                }
            }
            items(state.rooms, key = { it.id }) { room ->
                RoomCard(
                    room,
                    terminals.sessions.filter { it.missionId == room.id },
                    state.snapshots[room.id],
                    room.ownerUserId == account.userId,
                    room.unread || room.id in state.unread
                ) { open(room.id) }
            }
            if (state.truncated) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Text(
                        stringResource(R.string.rooms_truncated),
                        color = KodosiTheme.colors.inkMuted,
                        modifier = Modifier.padding(16.dp)
                    )
                }
            }
        }
    }
    if (creating) NewRoomDialog(model, dismiss = { creating = false }, open = open)
}

@Composable
private fun RoomCard(
    room: Room,
    sessions: List<RuntimeSession>,
    snapshot: RoomSnapshot?,
    own: Boolean,
    unread: Boolean,
    open: () -> Unit
) {
    val colors = KodosiTheme.colors
    val last = snapshot?.messages?.lastOrNull()
    KodosiCard(Modifier.fillMaxWidth(), onClick = open, radius = Radius.xl) {
        Column(Modifier.padding(16.dp).heightIn(min = 132.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
                RoomMark(room.id, 44.dp)
                Spacer(Modifier.weight(1f))
                if (unread) {
                    Box(Modifier.padding(top = 5.dp).size(7.dp).background(colors.accent, CircleShape))
                } else if (own) {
                    Text(stringResource(R.string.yours), style = KodosiType.caption, color = colors.inkFaint)
                }
            }
            Text(
                room.name,
                style = KodosiType.headline,
                color = colors.ink,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis
            )
            if (last != null || snapshot != null) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    val preview = last?.let {
                        "${it.agent ?: it.authorName}: ${it.text.lineSequence().joinToString(" ")}"
                    } ?: stringResource(R.string.no_messages)
                    Text(
                        preview,
                        Modifier.weight(1f),
                        style = KodosiType.footnote,
                        color = colors.inkMuted,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis
                    )
                    sessions.firstOrNull { it.needsUser }?.sign()?.let { StatusSign(it, 13.dp) }
                        ?: last?.let {
                            Text(roomTime(it.createdAt), style = KodosiType.caption, color = colors.inkFaint)
                        }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(horizontalArrangement = Arrangement.spacedBy((-4).dp)) {
                    sessions.distinctBy { it.agent to it.programAccount }.take(3).forEach {
                        AgentMark(
                            it.agent,
                            18.dp,
                            account = it.programAccount,
                            activity = if (it.isWorking) MarkActivity.WORKING else MarkActivity.AWAKE
                        )
                    }
                }
                Text(
                    if (sessions.isEmpty()) {
                        stringResource(R.string.no_terminals_yet)
                    } else {
                        pluralStringResource(R.plurals.terminal_count, sessions.size, sessions.size)
                    },
                    style = KodosiType.caption,
                    color = colors.inkFaint
                )
                if (sessions.any { it.isWorking }) {
                    Text(stringResource(R.string.working), style = KodosiType.caption, color = colors.inkMuted)
                }
            }
        }
    }
}

@Composable
internal fun roomTime(stamp: String): String {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    return remember(stamp, configuration) {
        runCatching {
            val time = Instant.parse(stamp).toEpochMilli()
            android.text.format.DateUtils.formatDateTime(
                context,
                time,
                if (android.text.format.DateUtils.isToday(time)) {
                    android.text.format.DateUtils.FORMAT_SHOW_TIME
                } else {
                    android.text.format.DateUtils.FORMAT_SHOW_DATE or android.text.format.DateUtils.FORMAT_ABBREV_MONTH
                }
            )
        }.getOrDefault("")
    }
}

@Composable
private fun NewRoomDialog(model: AppModel, dismiss: () -> Unit, open: (String) -> Unit) {
    var name by remember { mutableStateOf("") }
    var inviting by remember { mutableStateOf<Set<String>>(emptySet()) }
    var busy by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val people by model.people.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    KodosiDialog(
        onDismissRequest = { if (!busy) dismiss() },
        title = { Text(stringResource(R.string.new_room)) },
        text = {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                item {
                    KodosiField(name, {
                        name = it
                    }, label = stringResource(R.string.name), singleLine = true)
                }
                if (people.friends.isNotEmpty()) item { SectionTitle(stringResource(R.string.invite_friends)) }
                items(people.friends, key = { it.userId }) { person ->
                    CheckRow(person.name, person.userId in inviting, {
                        inviting = if (it) inviting + person.userId else inviting - person.userId
                    }, enabled = !busy, leading = { PersonAvatar(person.name, person.userId) })
                }
                failure?.let { item { ErrorNote(it) } }
                if (busy) item { ProgressCaption(stringResource(R.string.working)) }
            }
        },
        confirmButton = {
            KodosiButton(
                enabled = name.isNotBlank() && name.trim().encodeToByteArray().size <= 128 && !busy,
                onClick = {
                    busy = true
                    scope.launch {
                        try {
                            val id = model.rooms.create(name)
                            for (friend in inviting) {
                                model.runAction {
                                    model.commands.request(
                                        "mission.invite",
                                        fields(
                                            "missionId" to id,
                                            "userId" to friend
                                        )
                                    )
                                }
                            }
                            dismiss()
                            open(id)
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            failure = error.localizedMessage
                        } finally {
                            busy = false
                        }
                    }
                }
            ) { Text(stringResource(R.string.create)) }
        },
        dismissButton = {
            KodosiTextButton(enabled = !busy, onClick = dismiss) { Text(stringResource(R.string.cancel)) }
        }
    )
}
