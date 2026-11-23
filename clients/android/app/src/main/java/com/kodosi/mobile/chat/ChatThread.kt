package com.kodosi.mobile.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.terminals.agentLabel
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MarkActivity
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.SelectableWords
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.snappy
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch

private sealed interface ThreadEntry {
    val key: String

    data class Day(override val key: String, val at: Long) : ThreadEntry

    /** [opens] a run of messages by the same person, which shows its time under its last one, where it [closes]. */
    data class Person(
        val row: ChatRow,
        val words: ChatRow.Kind.Person,
        val mine: Boolean,
        val opens: Boolean,
        val closes: Boolean
    ) : ThreadEntry {
        override val key: String get() = row.id
    }

    /** The agent's mark, name and time over the rows of one turn. */
    data class Head(override val key: String, val at: Long, val working: Boolean) : ThreadEntry

    data class Agent(val row: ChatRow) : ThreadEntry {
        override val key: String get() = row.id
    }

    data class Compacted(val row: ChatRow) : ThreadEntry {
        override val key: String get() = row.id
    }

    data object Live : ThreadEntry {
        override val key = "live"
    }
}

/** The rows that the chat had when it came stay still; a row that comes later rises. */
private class Arrived {
    val keys = mutableSetOf<String>()
    private var settled = false

    fun settle(shown: List<String>) {
        if (settled) return
        keys += shown
        settled = true
    }

    fun fresh(key: String) = settled && key !in keys
}

/**
 * [me] is the account of this app: the messages of other people show their names and faces. A row
 * from before the agent reported to the chat has no time ([ChatItem.at] is 0) and starts no day.
 */
private fun entries(rows: List<ChatRow>, me: String?, working: Boolean, now: Boolean): List<ThreadEntry> = buildList {
    var anchor = "start"
    var grouped = false
    var at: Long? = null
    for (row in rows) {
        if (row.at > 0) {
            if (at?.let { ChatWords.sameDay(it, row.at) } != true) add(ThreadEntry.Day("day.${row.id}", row.at))
            at = row.at
        }
        when (val kind = row.kind) {
            is ChatRow.Kind.Person -> {
                grouped = false
                anchor = row.id
                val mine = me == null || kind.by == null || kind.by.id == me
                val previous = (lastOrNull() as? ThreadEntry.Person)
                    ?.takeIf { it.mine == mine && it.words.by?.id == kind.by?.id }
                if (previous != null) this[lastIndex] = previous.copy(closes = false)
                add(ThreadEntry.Person(row, kind, mine, opens = previous == null, closes = true))
            }

            is ChatRow.Kind.Note -> if (kind.note is ChatNote.Compacted) {
                grouped = false
                anchor = row.id
                add(ThreadEntry.Compacted(row))
            } else {
                if (!grouped) add(ThreadEntry.Head("head.$anchor", row.at, working = false))
                grouped = true
                add(ThreadEntry.Agent(row))
            }

            else -> {
                if (!grouped) add(ThreadEntry.Head("head.$anchor", row.at, working = false))
                grouped = true
                add(ThreadEntry.Agent(row))
            }
        }
    }
    if (working || now) {
        if (!grouped) {
            val started = rows.lastOrNull()?.at?.takeIf { it > 0 } ?: System.currentTimeMillis()
            add(ThreadEntry.Head("head.$anchor", started, working = false))
        }
        if (now) add(ThreadEntry.Live)
        val head = indexOfLast { it is ThreadEntry.Head }
        this[head] = (this[head] as ThreadEntry.Head).copy(working = working)
    }
}

/** What runs now shows while the agent works, asks nothing and writes no answer or plan, which its cursor shows. */
private fun showsNow(view: ChatView, rows: List<ChatRow>): Boolean {
    val writes = when (val last = rows.lastOrNull()?.kind) {
        is ChatRow.Kind.Answer -> last.live
        is ChatRow.Kind.Plan -> last.live
        else -> false
    }
    return view.state == ChatState.WORKING && view.asks.isEmpty() && !writes
}

/**
 * The newest message of a person starts a part as high as the view, so that after Send the
 * message moves to the top and the answer grows below it. The conversation follows its end until
 * the person scrolls back, and a drag down into the keyboard puts the keyboard away.
 */
@Composable
internal fun ChatThread(
    view: ChatView,
    agent: AgentKind,
    me: String?,
    sent: Int,
    hideKeyboard: () -> Unit,
    modifier: Modifier = Modifier
) {
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val rows = remember(view.items, view.working) { ChatRow.rows(view.items, view.working) }
    val working = view.state == ChatState.WORKING
    val now = showsNow(view, rows)
    val entries = remember(rows, me, working, now) { entries(rows, me, working, now) }
    val list = rememberLazyListState(initialFirstVisibleItemIndex = entries.size)
    var following by remember { mutableStateOf(true) }
    var tail by remember { mutableIntStateOf(0) }
    val end by rememberUpdatedState(entries.size)
    val newest by rememberUpdatedState(entries.indexOfLast { it is ThreadEntry.Person })
    val arrived = remember { Arrived() }
    val loaded = view.chat != null
    SideEffect { if (loaded) arrived.settle(entries.map { it.key }) }
    val byPerson = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (source == NestedScrollSource.UserInput && available.y > 0f) following = false
                return Offset.Zero
            }
        }
    }
    LaunchedEffect(list) {
        snapshotFlow { list.canScrollForward }.collect { if (!it) following = true }
    }
    LaunchedEffect(list) {
        snapshotFlow { following && list.canScrollForward && !list.isScrollInProgress }.collect {
            if (it) list.scrollToItem(end)
        }
    }
    LaunchedEffect(sent) {
        if (sent == 0) return@LaunchedEffect
        following = true
        list.animateScrollToItem(end)
    }
    LaunchedEffect(list) {
        snapshotFlow {
            val info = list.layoutInfo
            val visible = info.visibleItemsInfo
            val start = visible.firstOrNull { it.index == newest }
            val last = visible.firstOrNull { it.index == end - 1 }
            when {
                newest < 0 -> 0

                start != null && last != null ->
                    info.viewportEndOffset - info.viewportStartOffset - (last.offset + last.size - start.offset)

                last != null && (visible.firstOrNull()?.index ?: 0) > newest -> 0

                else -> null
            }
        }.filterNotNull().collect { tail = it.coerceAtLeast(0) }
    }
    val window = LocalView.current
    val density = LocalDensity.current
    val keyboard = WindowInsets.ime
    var top by remember { mutableFloatStateOf(0f) }
    Box(
        modifier.onGloballyPositioned { top = it.positionInRoot().y }.pointerInput(Unit) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                do {
                    val change = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == down.id }
                        ?: break
                    val raised = keyboard.getBottom(density)
                    val below = top + change.position.y >= window.height - raised
                    if (raised > 0 && below && change.position.y > change.previousPosition.y) {
                        hideKeyboard()
                        break
                    }
                } while (change.pressed)
            }
        }
    ) {
        LazyColumn(Modifier.fillMaxSize().nestedScroll(byPerson), state = list) {
            items(entries, key = { it.key }) { entry ->
                val fresh = remember { arrived.fresh(entry.key) }
                SideEffect { arrived.keys += entry.key }
                Rising(
                    fresh,
                    Modifier.fillMaxWidth().padding(horizontal = 14.dp).animateItem(
                        fadeInSpec = null,
                        placementSpec = snappy(),
                        fadeOutSpec = fade()
                    )
                ) {
                    Entry(entry, view, agent) { following = false }
                }
            }
            item(key = "tail") {
                Spacer(Modifier.height(with(density) { maxOf(tail, 12.dp.roundToPx()).toDp() }))
            }
        }
        if (loaded && rows.isEmpty() && view.state != ChatState.WORKING) {
            AgentMark(agent, 40.dp, Modifier.align(Alignment.Center))
        }
        val latest by remember { derivedStateOf { !following && list.canScrollForward } }
        val sliding = snappy<IntOffset>()
        val fading = fade<Float>()
        AnimatedVisibility(
            latest,
            Modifier.align(Alignment.BottomCenter).padding(8.dp),
            enter = fadeIn(fading) + slideInVertically(sliding) { it / 2 },
            exit = fadeOut(fading) + slideOutVertically(sliding) { it / 2 }
        ) {
            KodosiButton(
                onClick = {
                    following = true
                    scope.launch { list.animateScrollToItem(end) }
                },
                kind = ButtonKind.SECONDARY,
                size = ButtonSize.SMALL
            ) {
                AppIcon(AppSymbol.ARROW_DOWN, Modifier.size(11.dp), tint = colors.ink)
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.chat_latest))
            }
        }
    }
}

@Composable
private fun Entry(entry: ThreadEntry, view: ChatView, agent: AgentKind, hold: () -> Unit) {
    val colors = KodosiTheme.colors
    when (entry) {
        is ThreadEntry.Day ->
            DayCapsule(ChatWords.day(entry.at), Modifier.fillMaxWidth().padding(top = 16.dp, bottom = 4.dp))

        is ThreadEntry.Person -> if (entry.mine) Mine(entry) else Theirs(entry)

        is ThreadEntry.Head -> Row(Modifier.padding(top = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            AgentMark(agent, 24.dp, activity = if (entry.working) MarkActivity.WORKING else MarkActivity.AWAKE)
            Spacer(Modifier.width(8.dp))
            Text(agentLabel(agent), style = KodosiType.caption, color = colors.inkMuted, maxLines = 1)
            Spacer(Modifier.width(6.dp))
            if (entry.at > 0) {
                Text(ChatWords.time(entry.at), style = KodosiType.caption, color = colors.inkFaint, maxLines = 1)
            }
        }

        is ThreadEntry.Agent -> Box(Modifier.padding(start = AgentColumn, end = 12.dp, top = 6.dp)) {
            AgentRow(entry.row, hold)
        }

        is ThreadEntry.Compacted ->
            DayCapsule(stringResource(R.string.chat_compacted), Modifier.fillMaxWidth().padding(vertical = 12.dp))

        ThreadEntry.Live -> RollingText(
            ChatWords.now(view),
            KodosiType.caption,
            colors.inkMuted,
            Modifier.padding(start = AgentColumn, top = 8.dp, bottom = 4.dp)
        )
    }
}

private val AgentColumn = 32.dp

@Composable
private fun AgentRow(row: ChatRow, hold: () -> Unit) {
    val colors = KodosiTheme.colors
    when (val kind = row.kind) {
        is ChatRow.Kind.Work -> ChatWork(kind.items, hold)

        is ChatRow.Kind.Answer -> Answer { ChatProse(kind.text, kind.live) }

        is ChatRow.Kind.Plan -> Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                AppIcon(AppSymbol.TASKS, Modifier.size(11.dp), tint = colors.inkFaint)
                Spacer(Modifier.width(5.dp))
                Text(stringResource(R.string.chat_mode_plan), style = KodosiType.caption, color = colors.inkFaint)
            }
            Answer { ChatProse(kind.text, kind.live) }
        }

        is ChatRow.Kind.End -> Row(verticalAlignment = Alignment.CenterVertically) {
            val tag = when (kind.end) {
                TurnEnd.STOPPED -> R.string.chat_stopped
                TurnEnd.FAILED -> R.string.sign_failed
                TurnEnd.REFUSED -> R.string.chat_refused
                TurnEnd.DONE -> null
            }
            tag?.let {
                Capsule(stringResource(it))
                Spacer(Modifier.width(8.dp))
            }
            if (kind.ms >= ChatRow.LONG_TURN_MS) {
                Text(
                    stringResource(R.string.chat_worked_for, ChatWords.duration(kind.ms)),
                    style = KodosiType.caption,
                    color = colors.inkFaint
                )
            }
        }

        is ChatRow.Kind.Note -> when (val note = kind.note) {
            is ChatNote.Failed -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusSign(StatusForm.FAILED, 12.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(note.error, style = ChatType.monoCaption, color = colors.ink)
                }
                if (note.text.isNotEmpty()) SelectableWords(note.text)
            }

            ChatNote.SignedOut -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    StatusSign(StatusForm.KEY, 12.dp)
                    Spacer(Modifier.width(6.dp))
                    Text(stringResource(R.string.sign_key), style = KodosiType.caption, color = colors.ink)
                }
                Text(
                    stringResource(R.string.chat_signed_out_detail),
                    style = KodosiType.caption,
                    color = colors.inkMuted
                )
            }

            ChatNote.Compacted -> Unit
        }

        is ChatRow.Kind.Person -> Unit
    }
}

/** Android shows the agent's answer as its room shows another person's message. */
@Composable
private fun Answer(content: @Composable () -> Unit) {
    val colors = KodosiTheme.colors
    Box(
        Modifier.widthIn(max = 640.dp)
            .background(colors.raised, TheirsShape)
            .padding(horizontal = 12.dp, vertical = 8.dp)
    ) { content() }
}

@Composable
private fun Mine(entry: ThreadEntry.Person) {
    val colors = KodosiTheme.colors
    Column(
        Modifier.fillMaxWidth().padding(start = 48.dp, top = if (entry.opens) 16.dp else 4.dp),
        horizontalAlignment = Alignment.End,
        verticalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        SelectableWords(
            entry.words.text,
            Modifier.widthIn(max = 640.dp).background(colors.accentSoft, MineShape)
                .padding(horizontal = 12.dp, vertical = 8.dp),
            markdown = true
        )
        if (entry.closes && entry.row.at > 0) {
            Text(ChatWords.time(entry.row.at), style = KodosiType.caption, color = colors.inkFaint)
        }
    }
}

@Composable
private fun Theirs(entry: ThreadEntry.Person) {
    val colors = KodosiTheme.colors
    val person = entry.words.by
    Row(Modifier.fillMaxWidth().padding(end = 12.dp, top = if (entry.opens) 16.dp else 4.dp)) {
        if (entry.opens && person != null) {
            PersonAvatar(person.name, person.id, 24.dp)
        } else {
            Spacer(Modifier.width(24.dp))
        }
        Spacer(Modifier.width(8.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            if (entry.opens && person != null) {
                Text(person.name, style = KodosiType.caption, color = colors.inkMuted, maxLines = 1)
            }
            SelectableWords(
                entry.words.text,
                Modifier.widthIn(max = 640.dp).background(colors.raised, TheirsShape)
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                markdown = true
            )
            if (entry.closes && entry.row.at > 0) {
                Text(ChatWords.time(entry.row.at), style = KodosiType.caption, color = colors.inkFaint)
            }
        }
    }
}

private val MineShape = RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp, bottomStart = 14.dp, bottomEnd = 4.dp)
private val TheirsShape = RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp, bottomStart = 4.dp, bottomEnd = 14.dp)
