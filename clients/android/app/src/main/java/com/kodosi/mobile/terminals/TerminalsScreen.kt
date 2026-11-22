package com.kodosi.mobile.terminals

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
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
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
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
import com.kodosi.mobile.ui.Still
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.snappy

@Composable
fun TerminalsScreen(
    store: TerminalStore,
    onlineHosts: Set<String>,
    online: Boolean,
    summaryFirst: Boolean,
    open: (String) -> Unit,
    start: (NewTerminalRequest) -> Unit
) {
    val colors = KodosiTheme.colors
    val state by store.state.collectAsStateWithLifecycle()
    val query = rememberTextFieldState()
    var folded by rememberSaveable { mutableStateOf(emptySet<String>()) }
    val sessions = TerminalCatalog.matching(state.sessions.filter { !it.hidden }, query.text.toString())
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
        item { SearchField(query) }
        for (group in groups) {
            val own = group.place is TerminalGroup.Place.Computer
            item(key = "group-$own-${group.place.id}") {
                GroupHeader(
                    group,
                    if (own && group.place.id in onlineHosts) {
                        { start(NewTerminalRequest(computer = group.place.id)) }
                    } else {
                        null
                    },
                    Modifier.animateItem(fade(), snappy(), fade())
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
                            modifier = Modifier.animateItem(fade(), snappy(), fade()).padding(top = 12.dp)
                        )
                    }
                }
                for ((folder, entries) in terminals.groupBy { it.workingDir }) {
                    val key = "${group.place.id}/$host/$folder"
                    if (folder != null) {
                        item(key = "folder-$key") {
                            Row(
                                Modifier.animateItem(fade(), snappy(), fade()).fillMaxWidth().heightIn(min = 48.dp)
                                    .combinedClickable(
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
                    if (key !in folded || query.text.isNotBlank()) {
                        items(entries, key = { it.id }) { session ->
                            TerminalCard(session, online, summaryFirst = summaryFirst, onClick = {
                                open(session.id)
                            }, modifier = Modifier.animateItem(fade(), snappy(), fade()).padding(vertical = 5.dp))
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

                    query.text.isNotBlank() -> Text(
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
private fun SearchField(query: TextFieldState) {
    KodosiField(
        query,
        placeholder = stringResource(R.string.search_terminals),
        lineLimits = TextFieldLineLimits.SingleLine,
        modifier = Modifier.fillMaxWidth(),
        leading = {
            Glyph(R.drawable.ic_search, KodosiTheme.colors.inkMuted, 18.dp)
        }
    )
}

@Composable
private fun GroupHeader(group: TerminalGroup, start: (() -> Unit)?, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier.fillMaxWidth().padding(top = 18.dp, bottom = 4.dp).semantics(mergeDescendants = true) {
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

@Composable
fun TerminalCard(
    session: RuntimeSession,
    online: Boolean,
    summaryFirst: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
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
                val (title, detail) = session.cardLines(online, summaryFirst)
                Text(
                    title,
                    style = KodosiType.subhead,
                    color = if (session.canOpen) colors.ink else colors.inkMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
                TerminalCaption(session, online, detail)
            }
            Spacer(Modifier.width(8.dp))
            when {
                sign != null -> StatusSign(sign, 13.dp)
                session.isTroubled -> Box(Modifier.size(6.dp).background(colors.caution, CircleShape))
            }
        }
    }
}

internal enum class CaptionState { OFFLINE, TROUBLED, RECONNECTING, CLOSING, NOTICE }

internal fun RuntimeSession.captionState(online: Boolean): CaptionState? {
    val troubled = isTroubled || connectionState == RuntimeSession.Connection.CONNECTING
    val reconnecting = status == RuntimeSession.Status.RECONNECTING
    val closing = status == RuntimeSession.Status.CLOSING
    return when {
        unreachable(online) && (isConnected || troubled || reconnecting || closing) -> CaptionState.OFFLINE
        troubled -> CaptionState.TROUBLED
        reconnecting -> CaptionState.RECONNECTING
        closing -> CaptionState.CLOSING
        message != null && isConnected -> CaptionState.NOTICE
        else -> null
    }
}

/** While the second line says a state, the name stays the title, so that the card still names its terminal. */
internal fun RuntimeSession.cardLines(online: Boolean, summaryFirst: Boolean): Pair<String, String?> {
    val summary = activity?.takeIf { summaryFirst && agent.isAgent && captionState(online) == null }
    return if (summary != null) summary to name else name to activity
}

@Composable
fun TerminalCaption(session: RuntimeSession, online: Boolean, detail: String? = session.activity) {
    val (words, color) = captionOf(session, online, detail)
    Text(words, style = KodosiType.footnote, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

@Composable
fun captionOf(session: RuntimeSession, online: Boolean, detail: String? = session.activity): Pair<String, Color> {
    val colors = KodosiTheme.colors
    return when (session.captionState(online)) {
        CaptionState.OFFLINE -> stringResource(R.string.offline) to colors.caution

        CaptionState.TROUBLED -> statusLabel(session) to if (session.isTroubled) colors.caution else colors.inkFaint

        CaptionState.RECONNECTING -> stringResource(R.string.reconnecting) to colors.caution

        CaptionState.CLOSING -> statusLabel(session) to colors.inkFaint

        CaptionState.NOTICE -> session.message.orEmpty() to colors.caution

        null -> (detail ?: session.folderName ?: agentLabel(session.agent)) to
            if (detail == null) colors.inkFaint else colors.inkMuted
    }
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
    AgentKind.ANTIGRAVITY -> stringResource(R.string.agent_antigravity)
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
        Still {
            AgentMark(
                AgentKind.CLAUDE,
                34.dp,
                activity = MarkActivity.WORKING,
                modifier = Modifier.align(Alignment.BottomEnd)
            )
        }
    }
}
