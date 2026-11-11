package com.kodosi.mobile.terminals

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
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
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.DarkColors
import com.kodosi.mobile.ui.Glyph
import com.kodosi.mobile.ui.KodosiCard
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiSpinner
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.raised

/** The Terminals tab: your terminals by computer, then the terminals that friends share. */
@Composable
fun TerminalsScreen(
    store: TerminalStore,
    onlineHosts: Set<String>,
    online: Boolean,
    open: (String) -> Unit,
    start: (NewTerminalRequest) -> Unit
) {
    val colors = KodosiTheme.colors
    val state by store.state.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    var folded by rememberSaveable { mutableStateOf(emptySet<String>()) }
    val sessions = TerminalCatalog.matching(state.sessions.filter { !it.hidden }, query)
    val groups = TerminalCatalog.groups(
        sessions,
        stringResource(R.string.remote_computer),
        stringResource(R.string.shared_with_you)
    )
    LazyColumn(
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
        modifier = Modifier.fillMaxSize().safeDrawingPadding()
    ) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                Text(
                    stringResource(R.string.terminals),
                    style = KodosiType.large,
                    color = colors.ink,
                    modifier = Modifier.weight(1f).padding(top = 24.dp, bottom = 14.dp).semantics { heading() }
                )
                ActionIcon(AppSymbol.ADD, R.string.new_terminal, { start(NewTerminalRequest()) })
            }
        }
        item { SearchField(query) { query = it } }
        for (group in groups) {
            val own = group.place is TerminalGroup.Place.Computer
            item(key = "group-$own-${group.place.id}") {
                GroupHeader(
                    group,
                    if (own && group.place.id in onlineHosts) {
                        { start(NewTerminalRequest(computer = group.place.id)) }
                    } else {
                        null
                    }
                )
            }
            val hosts = group.sessions.groupBy { it.hostDeviceId }
            for ((host, terminals) in hosts) {
                if (!own && hosts.size > 1) {
                    item(key = "computer-${group.place.id}-$host") {
                        Text(
                            terminals.first().hostName.orEmpty(),
                            style = KodosiType.footnote,
                            color = colors.inkMuted,
                            modifier = Modifier.padding(top = 12.dp)
                        )
                    }
                }
                for ((folder, entries) in terminals.groupBy { it.workingDir }) {
                    val key = "${group.place.id}/$host/$folder"
                    if (folder != null) {
                        item(key = "folder-$key") {
                            Row(
                                Modifier.fillMaxWidth().heightIn(min = 48.dp).combinedClickable(
                                    onClick = { folded = if (key in folded) folded - key else folded + key },
                                    onLongClick = if (own && host in onlineHosts) {
                                        { start(NewTerminalRequest(host, folder)) }
                                    } else {
                                        null
                                    }
                                ),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp)
                            ) {
                                AppIcon(AppSymbol.FOLDER, tint = colors.inkFaint)
                                Text(
                                    folder.trimEnd('/', '\\').substringAfterLast('/').substringAfterLast('\\').ifEmpty {
                                        folder
                                    },
                                    style = KodosiType.footnote,
                                    color = colors.inkMuted,
                                    modifier = Modifier.weight(1f),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                                Text(entries.size.toString(), style = KodosiType.caption, color = colors.inkFaint)
                            }
                        }
                    }
                    if (key !in folded || query.isNotBlank()) {
                        items(entries, key = { it.id }) { session ->
                            TerminalCard(session, online, onClick = {
                                open(session.id)
                            }, modifier = Modifier.padding(vertical = 5.dp))
                        }
                    }
                }
            }
        }
        if (sessions.isEmpty()) {
            item {
                when {
                    !state.loaded || !state.listed -> Box(
                        Modifier.fillMaxWidth().padding(top = 64.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        KodosiSpinner(Modifier.size(22.dp))
                    }

                    query.isNotBlank() -> Text(
                        stringResource(R.string.nothing_matches),
                        style = KodosiType.footnote,
                        color = colors.inkMuted,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth().padding(top = 40.dp)
                    )

                    else -> EmptyTerminals()
                }
            }
        }
    }
}

@Composable
private fun SearchField(query: String, change: (String) -> Unit) {
    KodosiField(
        query,
        change,
        placeholder = stringResource(R.string.search_terminals),
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
        leading = {
            Glyph(R.drawable.ic_search, KodosiTheme.colors.inkMuted, 18.dp)
        }
    )
}

@Composable
private fun GroupHeader(group: TerminalGroup, start: (() -> Unit)?) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(top = 18.dp, bottom = 4.dp).semantics(mergeDescendants = true) {
            heading()
        }
    ) {
        val glyph = if (group.place is TerminalGroup.Place.Friend) R.drawable.ic_person else R.drawable.ic_laptop
        Glyph(glyph, if (group.away) colors.inkFaint else colors.inkMuted, 18.dp)
        Spacer(Modifier.width(7.dp))
        Text(
            group.place.name,
            style = KodosiType.subhead,
            color = colors.inkMuted,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
        Spacer(Modifier.width(7.dp))
        Text(group.sessions.size.toString(), style = KodosiType.caption, color = colors.inkFaint)
        if (start != null) ActionIcon(AppSymbol.ADD, R.string.new_terminal, start)
    }
}

/** A terminal in a list: its mark, its name, what it does, and its sign. */
@Composable
fun TerminalCard(session: RuntimeSession, online: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val sign = session.sign()
    KodosiCard(modifier.fillMaxWidth(), onClick = onClick) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.heightIn(min = 68.dp).padding(horizontal = 14.dp, vertical = 10.dp)
        ) {
            AgentMark(
                session.agent,
                34.dp,
                account = session.programAccount,
                activity = when {
                    session.isWorking -> MarkActivity.WORKING
                    session.canOpen -> MarkActivity.AWAKE
                    else -> MarkActivity.ASLEEP
                }
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    session.name,
                    style = KodosiType.subhead,
                    color = if (session.canOpen) colors.ink else colors.inkMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                TerminalCaption(session, online)
            }
            Spacer(Modifier.width(8.dp))
            when {
                sign != null -> StatusSign(sign, 13.dp)
                session.isTroubled -> Box(Modifier.size(6.dp).background(colors.caution, CircleShape))
            }
        }
    }
}

@Composable
fun TerminalCaption(session: RuntimeSession, online: Boolean) {
    val colors = KodosiTheme.colors
    val troubled = session.isTroubled || session.connectionState == RuntimeSession.Connection.CONNECTING
    val reconnecting = session.status == RuntimeSession.Status.RECONNECTING
    val closing = session.status == RuntimeSession.Status.CLOSING
    val notice = session.message?.takeIf { session.isConnected }
    val (words, color) = when {
        session.unreachable(online) && (session.isConnected || troubled || reconnecting || closing) ->
            stringResource(R.string.offline) to colors.caution

        troubled -> statusLabel(session) to if (session.isTroubled) colors.caution else colors.inkFaint

        reconnecting -> stringResource(R.string.reconnecting) to colors.caution

        closing -> statusLabel(session) to colors.inkFaint

        notice != null -> notice to colors.caution

        else -> (session.activity ?: session.folderName ?: agentLabel(session.agent)) to
            if (session.activity == null) colors.inkFaint else colors.inkMuted
    }
    Text(words, style = KodosiType.footnote, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

@Composable
fun statusLabel(session: RuntimeSession): String = when {
    session.connectionState == RuntimeSession.Connection.BLOCKED -> stringResource(R.string.no_access)

    session.status == RuntimeSession.Status.OFFLINE ->
        stringResource(
            R.string.host_offline,
            session.hostName ?: session.ownerName ?: stringResource(R.string.remote_computer)
        )

    session.connectionState == RuntimeSession.Connection.OFFLINE -> stringResource(R.string.not_connected)

    session.connectionState == RuntimeSession.Connection.CONNECTING -> stringResource(R.string.connecting)

    session.status == RuntimeSession.Status.RUNNING -> stringResource(R.string.running)

    session.status == RuntimeSession.Status.RECONNECTING -> stringResource(R.string.reconnecting)

    else -> stringResource(R.string.closing)
}

@Composable
fun agentLabel(kind: AgentKind): String = when (kind) {
    AgentKind.CLAUDE -> stringResource(R.string.agent_claude)
    AgentKind.CODEX -> stringResource(R.string.agent_codex)
    AgentKind.COPILOT -> stringResource(R.string.agent_copilot)
    AgentKind.CURSOR -> stringResource(R.string.agent_cursor)
    AgentKind.GEMINI -> stringResource(R.string.agent_gemini)
    AgentKind.OPENCODE -> stringResource(R.string.agent_opencode)
    AgentKind.SHELL -> stringResource(R.string.agent_shell)
}

@Composable
private fun EmptyTerminals() {
    val colors = KodosiTheme.colors
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxWidth().padding(top = 64.dp)
    ) {
        TerminalsArt()
        Spacer(Modifier.size(6.dp))
        Text(
            stringResource(R.string.terminals_empty_title),
            style = KodosiType.headline,
            color = colors.ink,
            textAlign = TextAlign.Center
        )
        Text(
            stringResource(R.string.terminals_empty_message),
            style = KodosiType.callout,
            color = colors.inkMuted,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 340.dp)
        )
    }
}

@Composable
private fun TerminalsArt() {
    val colors = DarkColors
    val shape = RoundedCornerShape(Radius.tile)
    Box(Modifier.size(142.dp, 98.dp)) {
        Canvas(Modifier.size(128.dp, 84.dp).raised(colors, shape, fill = colors.terminal)) {
            repeat(3) { drawCircle(colors.hairline, 3.dp.toPx(), Offset((17 + it * 10).dp.toPx(), 17.dp.toPx())) }
            val prompt = Path().apply {
                moveTo(14.dp.toPx(), 34.dp.toPx())
                lineTo(19.dp.toPx(), 39.dp.toPx())
                lineTo(14.dp.toPx(), 44.dp.toPx())
            }
            drawPath(prompt, colors.inkMuted, style = Stroke(2.dp.toPx(), cap = StrokeCap.Round))
            drawRoundRect(
                colors.accent,
                Offset(26.dp.toPx(), 32.dp.toPx()),
                Size(8.dp.toPx(), 15.dp.toPx()),
                CornerRadius(1.dp.toPx())
            )
        }
        AgentMark(
            AgentKind.CLAUDE,
            34.dp,
            activity = MarkActivity.WORKING,
            modifier = Modifier.align(Alignment.BottomEnd)
        )
    }
}
