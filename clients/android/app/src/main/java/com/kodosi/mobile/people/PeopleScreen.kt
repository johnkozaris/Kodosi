package com.kodosi.mobile.people

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.clippedUtf8
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.TerminalCard
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiCard
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiSpinner
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.SectionTitle
import com.kodosi.mobile.ui.TextEntryDialog
import com.kodosi.mobile.ui.raised
import kotlin.math.floor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
fun PeopleScreen(model: AppModel, back: () -> Unit, open: (String) -> Unit) {
    val state by model.people.state.collectAsStateWithLifecycle()
    val terminals by model.terminals.state.collectAsStateWithLifecycle()
    val opened by model.friendInvite.collectAsStateWithLifecycle()
    var adding by remember { mutableStateOf(false) }
    var showingInvite by remember { mutableStateOf(false) }
    var accepting by remember { mutableStateOf<Person?>(null) }
    var reporting by remember { mutableStateOf<Person?>(null) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(opened) {
        val invite = opened ?: return@LaunchedEffect
        model.friendInvite.value = null
        adding = false
        showingInvite = false
        accepting = Person("", invite, "")
    }
    Column(Modifier.fillMaxSize()) {
        PageHeader(stringResource(R.string.friends), back, actions = {
            ActionIcon(AppSymbol.REFRESH, R.string.refresh, model.people::refresh)
            ActionIcon(AppSymbol.QR_CODE, R.string.show_invite, { showingInvite = true })
            ActionIcon(AppSymbol.ADD, R.string.add_friend, { adding = true })
        })
        LazyColumn(Modifier.weight(1f).padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            if (!state.loaded) item { ProgressCaption(stringResource(R.string.loading), Modifier.padding(24.dp)) }
            if (state.incoming.isNotEmpty()) item { SectionTitle(stringResource(R.string.incoming_requests)) }
            items(state.incoming, key = { "incoming-${it.userId}" }) { person ->
                KodosiCard(radius = Radius.xl) {
                    ListLine(person.name, subtitle = "@${person.handle}", leading = {
                        PersonAvatar(person.name, person.userId, 40.dp)
                    }) {
                        KodosiTextButton(onClick = { reporting = person }) {
                            Text(stringResource(R.string.report), color = KodosiTheme.colors.danger)
                        }
                        KodosiTextButton(onClick = {
                            scope.launch {
                                model.runAction { model.people.act("friends.request.reject", person.handle) }
                            }
                        }) { Text(stringResource(R.string.decline)) }
                        KodosiButton(onClick = { accepting = person }) { Text(stringResource(R.string.accept)) }
                    }
                }
            }
            if (state.friends.isNotEmpty()) item { SectionTitle(stringResource(R.string.friends)) }
            items(state.friends, key = { it.userId }) { person ->
                FriendCard(person, terminals.sessions.filter { it.ownerUserId == person.userId && !it.hidden }) {
                    open(person.userId)
                }
            }
            if (state.outgoing.isNotEmpty()) item { SectionTitle(stringResource(R.string.outgoing_requests)) }
            items(state.outgoing, key = { "outgoing-${it.userId}" }) { person ->
                ListLine(person.name, subtitle = "@${person.handle}") {
                    KodosiTextButton(onClick = {
                        scope.launch { model.runAction { model.people.act("friends.request.cancel", person.handle) } }
                    }) {
                        Text(stringResource(R.string.cancel_request))
                    }
                }
            }
            if (state.blocked.isNotEmpty()) item { SectionTitle(stringResource(R.string.blocked_people)) }
            items(state.blocked, key = { "blocked-${it.userId}" }) { person ->
                ListLine(person.name, subtitle = "@${person.handle}") {
                    KodosiTextButton(onClick = {
                        scope.launch { model.runAction { model.people.act("friends.unblock", person.handle) } }
                    }) {
                        Text(stringResource(R.string.unblock))
                    }
                }
            }
            if (state.loaded && state.friends.isEmpty() && state.incoming.isEmpty() && state.outgoing.isEmpty()) {
                item {
                    EmptyState(
                        stringResource(R.string.friends_empty_title),
                        stringResource(R.string.friends_empty_message),
                        AppSymbol.FRIENDS
                    ) {
                        KodosiButton(stringResource(R.string.add_friend), { adding = true })
                        KodosiButton(
                            stringResource(R.string.show_invite),
                            { showingInvite = true },
                            kind = ButtonKind.SECONDARY
                        )
                    }
                }
            }
        }
    }
    if (adding) {
        AddFriendDialog(dismiss = { adding = false }) { text ->
            adding = false
            // The confirmation names the trust being granted, before the runtime resolves the handle.
            accepting = Person("", text, "")
        }
    }
    if (showingInvite) InviteDialog(model) { showingInvite = false }
    reporting?.let { ReportDialog(model, it) { reporting = null } }
    accepting?.let { person ->
        ConfirmDialog(
            if (person.userId.isEmpty()) "@" + PeopleStore.handle(person.handle).removePrefix("@") else person.name,
            stringResource(R.string.friend_trust_message),
            stringResource(if (person.userId.isEmpty()) R.string.add_friend else R.string.accept),
            { accepting = null },
            {
                scope.launch {
                    model.runAction {
                        model.people.act(
                            if (person.userId.isEmpty()) "friends.request.send" else "friends.request.accept",
                            person.handle
                        )
                        accepting = null
                    }
                }
            }
        )
    }
}

@Composable
private fun AddFriendDialog(dismiss: () -> Unit, add: (String) -> Unit) {
    val context = LocalContext.current
    val camera = remember(context) { canScanInvites(context) }
    var text by remember { mutableStateOf("") }
    var scanning by remember { mutableStateOf(false) }
    KodosiDialog(
        onDismissRequest = dismiss,
        title = { Text(stringResource(R.string.add_friend)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                KodosiField(
                    text,
                    { text = it },
                    label = stringResource(R.string.username_invite),
                    modifier = Modifier.fillMaxWidth()
                )
                if (camera) {
                    KodosiTextButton(onClick = { scanning = true }) {
                        AppIcon(AppSymbol.QR_CODE, tint = KodosiTheme.colors.accent)
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.scan_invite))
                    }
                }
            }
        },
        confirmButton = {
            KodosiButton(stringResource(R.string.add_friend), { add(text.trim()) }, enabled = text.isNotBlank())
        },
        dismissButton = { KodosiTextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } }
    )
    if (scanning) {
        InviteScanDialog(found = { invite ->
            scanning = false
            text = invite
            add(invite)
        }, dismiss = { scanning = false })
    }
}

@Composable
private fun ReportDialog(model: AppModel, person: Person, dismiss: () -> Unit) {
    val received = stringResource(R.string.report_received)
    var note by remember { mutableStateOf("") }
    var sending by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    KodosiDialog(
        onDismissRequest = { if (!sending) dismiss() },
        title = { Text(stringResource(R.string.report_person, person.name)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.report_message))
                KodosiField(
                    note,
                    { note = it.clippedUtf8(REPORT_NOTE_BYTES) },
                    Modifier.fillMaxWidth(),
                    label = stringResource(R.string.report_note),
                    enabled = !sending,
                    minLines = 3
                )
                failure?.let { ErrorNote(it) }
                if (sending) ProgressCaption(stringResource(R.string.working))
            }
        },
        confirmButton = {
            KodosiButton(stringResource(R.string.report), {
                failure = null
                sending = true
                scope.launch {
                    try {
                        model.people.report(person.handle, note)
                        dismiss()
                        model.notify(received)
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (error: Exception) {
                        failure = error.localizedMessage ?: model.words.didNotWork
                    } finally {
                        sending = false
                    }
                }
            }, kind = ButtonKind.DANGER, enabled = !sending)
        },
        dismissButton = {
            KodosiTextButton(onClick = dismiss, enabled = !sending) { Text(stringResource(R.string.cancel)) }
        }
    )
}

private const val REPORT_NOTE_BYTES = 1000

@Composable
private fun InviteDialog(model: AppModel, dismiss: () -> Unit) {
    val context = LocalContext.current
    val label = stringResource(R.string.your_invite)
    var invite by remember { mutableStateOf<Invite?>(null) }
    var failed by remember { mutableStateOf(false) }
    var attempt by remember { mutableIntStateOf(0) }
    LaunchedEffect(attempt) {
        failed = false
        try {
            invite = model.people.invite()
            failed = invite == null
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            failed = true
        }
    }
    KodosiDialog(
        onDismissRequest = dismiss,
        title = { Text(label) },
        text = {
            Column(
                Modifier.verticalScroll(rememberScrollState()),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                InviteCode(invite?.code, failed) { attempt++ }
                Text(stringResource(R.string.invite_code_hint), textAlign = TextAlign.Center)
            }
        },
        confirmButton = {
            KodosiButton(stringResource(R.string.share), {
                invite?.let {
                    context.startActivity(
                        Intent.createChooser(
                            Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it.text),
                            null
                        )
                    )
                }
            }, enabled = invite != null)
        },
        dismissButton = {
            KodosiTextButton(onClick = {
                invite?.let {
                    context.getSystemService(ClipboardManager::class.java)
                        .setPrimaryClip(ClipData.newPlainText(label, it.text))
                }
            }, enabled = invite != null) { Text(stringResource(R.string.copy_invite)) }
        }
    )
}

/** The card stays light in both appearances, as a printed code does, so that each camera reads its
 *  dark modules. */
@Composable
private fun InviteCode(rows: List<String>?, failed: Boolean, retry: () -> Unit) {
    val description = stringResource(R.string.invite_code_description)
    KodosiTheme(dark = false) {
        val colors = KodosiTheme.colors
        Box(
            Modifier.widthIn(max = 360.dp).fillMaxWidth().aspectRatio(1f)
                .raised(colors, RoundedCornerShape(Radius.xl)),
            contentAlignment = Alignment.Center
        ) {
            when {
                !rows.isNullOrEmpty() -> Canvas(
                    Modifier.fillMaxSize().semantics {
                        contentDescription = description
                        role = Role.Image
                    }
                ) {
                    val module = floor(size.minDimension / (rows.size + 2 * QUIET_ZONE))
                    val left = floor((size.width - module * rows.size) / 2)
                    val top = floor((size.height - module * rows.size) / 2)
                    val dark = Path()
                    rows.forEachIndexed { y, row ->
                        row.forEachIndexed { x, cell ->
                            if (cell == '1') {
                                dark.addRect(Rect(Offset(left + x * module, top + y * module), Size(module, module)))
                            }
                        }
                    }
                    drawPath(dark, colors.ink)
                }

                failed -> KodosiTextButton(onClick = retry) { Text(stringResource(R.string.try_again)) }

                else -> KodosiSpinner(Modifier.size(20.dp))
            }
        }
    }
}

private const val QUIET_ZONE = 4

@Composable
private fun FriendCard(person: Person, shared: List<RuntimeSession>, open: (() -> Unit)? = null) {
    val colors = KodosiTheme.colors
    KodosiCard(Modifier.fillMaxWidth(), onClick = open, radius = Radius.xl) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                PersonAvatar(person.name, person.userId, 46.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            person.name,
                            Modifier.weight(1f, fill = false),
                            style = KodosiType.headline,
                            color = colors.ink,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                        if (person.trusted) {
                            AppIcon(
                                AppSymbol.CHECK,
                                Modifier.size(12.dp),
                                description = stringResource(R.string.verified),
                                tint = colors.ready
                            )
                        }
                    }
                    Text("@${person.handle}", style = KodosiType.footnote, color = colors.inkMuted)
                }
                if (open != null) AppIcon(AppSymbol.FORWARD, Modifier.size(11.dp).padding(top = 4.dp))
            }
            if (person.identityState == "changed") {
                Text(stringResource(R.string.identity_changed), style = KodosiType.footnote, color = colors.caution)
            } else if (shared.isEmpty()) {
                Text(stringResource(R.string.nothing_shared), style = KodosiType.footnote, color = colors.inkFaint)
            } else {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    shared.take(4).forEach {
                        AgentMark(
                            it.agent,
                            24.dp,
                            account = it.programAccount,
                            activity = if (it.isWorking) MarkActivity.WORKING else MarkActivity.AWAKE
                        )
                    }
                    Text(
                        pluralStringResource(R.plurals.terminal_count, shared.size, shared.size),
                        style = KodosiType.footnote,
                        color = colors.inkMuted
                    )
                }
            }
        }
    }
}

@Composable
fun FriendScreen(model: AppModel, id: String, back: () -> Unit, terminal: (String) -> Unit) {
    val people by model.people.state.collectAsStateWithLifecycle()
    val terminals by model.terminals.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val person = people.friends.firstOrNull { it.userId == id }
    var verifying by remember { mutableStateOf(false) }
    var confirming by remember { mutableStateOf<String?>(null) }
    var reporting by remember { mutableStateOf<Person?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.fillMaxSize()) {
        PageHeader(person?.name ?: stringResource(R.string.friends), back)
        if (person == null) return@Column
        LazyColumn(Modifier.weight(1f).padding(horizontal = 16.dp)) {
            item {
                FriendCard(person, terminals.sessions.filter { it.ownerUserId == id && !it.hidden })
            }
            item { KodosiTextButton(onClick = { verifying = true }) { Text(stringResource(R.string.verify_identity)) } }
            if (person.identityState == "changed") {
                item {
                    KodosiTextButton(onClick = {
                        confirming = "friends.identity.trust"
                    }) { Text(stringResource(R.string.trust_identity)) }
                }
            }
            item { SectionTitle(stringResource(R.string.terminals)) }
            items(terminals.sessions.filter { it.ownerUserId == id && !it.hidden }, key = { it.id }) { session ->
                TerminalCard(session, account.online, { terminal(session.id) }, Modifier.padding(vertical = 5.dp))
            }
            item {
                KodosiTextButton(onClick = {
                    confirming = "friends.remove"
                }) { Text(stringResource(R.string.remove_friend), color = KodosiTheme.colors.danger) }
                KodosiTextButton(onClick = {
                    confirming = "friends.block"
                }) { Text(stringResource(R.string.block), color = KodosiTheme.colors.danger) }
                KodosiTextButton(onClick = {
                    reporting = person
                }) { Text(stringResource(R.string.report), color = KodosiTheme.colors.danger) }
            }
        }
    }
    reporting?.let { ReportDialog(model, it) { reporting = null } }
    if (verifying && person != null) {
        TextEntryDialog(
            stringResource(R.string.verify_identity),
            stringResource(R.string.verify_invite),
            action = stringResource(R.string.verify_identity),
            dismiss = { verifying = false }
        ) { model.people.act("friends.verify", person.handle, it) }
    }
    if (person != null) {
        confirming?.let { operation ->
            val title = when (operation) {
                "friends.block" -> R.string.block
                "friends.remove" -> R.string.remove_friend
                else -> R.string.trust_identity
            }
            ConfirmDialog(
                stringResource(title),
                stringResource(
                    if (operation ==
                        "friends.block"
                    ) {
                        R.string.block_message
                    } else if (operation ==
                        "friends.remove"
                    ) {
                        R.string.remove_friend_message
                    } else {
                        R.string.friend_trust_message
                    }
                ),
                stringResource(title),
                { confirming = null },
                {
                    scope.launch {
                        model.runAction {
                            model.people.act(operation, person.handle)
                            confirming = null
                            if (operation != "friends.identity.trust") back()
                        }
                    }
                },
                destructive = operation != "friends.identity.trust"
            )
        }
    }
}
