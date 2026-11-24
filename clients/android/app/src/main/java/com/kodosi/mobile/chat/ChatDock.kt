package com.kodosi.mobile.chat

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layoutId
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.terminals.agentLabel
import com.kodosi.mobile.terminals.effortLabel
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MessageComposer
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.StatusSign
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.snappy
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

/** What the facts row shows of the agent's models: their names and, for each, its levels of effort. */
internal class KnownModels(val label: (String) -> String, val levels: (String?) -> List<String>)

/**
 * What sits under the conversation: the agent's list of its work, the waiting messages, the
 * question of the agent, the composer and the facts of the session.
 */
@Composable
internal fun ChatDock(
    chat: TerminalChat,
    view: ChatView,
    agent: AgentKind,
    draft: TextFieldState,
    models: KnownModels,
    send: () -> Unit,
    showTerminal: () -> Unit,
    modelKey: @Composable (label: String, open: Boolean, setOpen: (Boolean) -> Unit) -> Unit
) {
    val colors = KodosiTheme.colors
    val tasks = view.tasks.takeIf { list -> list.any { it.state != TaskState.DONE } }.orEmpty()
    val leaving = remember { mutableStateOf(tasks) }
    SideEffect { if (tasks.isNotEmpty()) leaving.value = tasks }
    var modelOpen by remember { mutableStateOf(false) }
    var usageOpen by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val keyboard = LocalSoftwareKeyboardController.current
    val open: (ChatControl) -> Unit = { control ->
        focus.clearFocus()
        keyboard?.hide()
        when (control) {
            ChatControl.MODEL -> modelOpen = true
            ChatControl.USAGE -> usageOpen = true
        }
    }
    val choose: (ChatCommand) -> Unit = { command ->
        val control = command.opens
        if (control != null) {
            draft.clearText()
            open(control)
        } else {
            draft.setTextAndPlaceCursorAtEnd("/${command.name} ")
        }
    }
    val suggestions = ChatCommandMatch.commands(view.commands, draft.text.toString())
    val shownSuggestions = remember { mutableStateOf(suggestions) }
    SideEffect { if (suggestions.isNotEmpty()) shownSuggestions.value = suggestions }
    val ask = view.asks.firstOrNull()
    val fading = fade<Float>()
    val rising = snappy<IntOffset>()
    val sizing = snappy<IntSize>()
    val drop = with(LocalDensity.current) { 8.dp.roundToPx() }
    DockLayout(
        above = {
            Unfolding(tasks.isNotEmpty()) {
                ChatTasks(tasks.ifEmpty { leaving.value }, Modifier.padding(horizontal = 10.dp, vertical = 4.dp))
            }
            view.queued.forEach { queued -> Queued(chat, queued, agent, showTerminal) }
        },
        card = {
            AnimatedContent(
                ask,
                transitionSpec = {
                    (fadeIn(fading) + slideInVertically(rising) { it / 3 }) togetherWith
                        (fadeOut(tween(120)) + slideOutVertically(rising) { drop }) using
                        SizeTransform(clip = false) { _, _ -> sizing }
                },
                contentKey = { it?.id },
                label = "question"
            ) { shown ->
                if (shown != null) {
                    Box(Modifier.padding(horizontal = 10.dp, vertical = 4.dp)) { ChatAskCard(chat, view, shown, agent) }
                }
            }
        },
        below = {
            Unfolding(view.failure) { failure ->
                Row(
                    Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    StatusSign(StatusForm.FAILED, 11.dp)
                    Spacer(Modifier.width(7.dp))
                    Text(failure, style = KodosiType.footnote, color = colors.inkMuted, maxLines = 2)
                }
            }
            if (view.chat != null && view.state == ChatState.AWAY) {
                Text(
                    stringResource(R.string.chat_no_agent),
                    style = KodosiType.footnote,
                    color = colors.inkFaint,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().navigationBarsPadding().padding(vertical = 14.dp)
                )
            } else {
                Unfolding(suggestions.isNotEmpty()) {
                    ChatCommandList(
                        suggestions.ifEmpty { shownSuggestions.value },
                        choose,
                        Modifier.padding(start = 10.dp, end = 10.dp, top = 4.dp)
                    )
                }
                MessageComposer(
                    draft,
                    busy = false,
                    send = {
                        val control = view.control(draft.text.toString())
                        if (control != null && view.canWrite) {
                            draft.clearText()
                            open(control)
                        } else {
                            send()
                        }
                    },
                    enabled = view.canWrite,
                    placeholder = if (view.question != null) {
                        stringResource(R.string.chat_say_instead)
                    } else {
                        stringResource(R.string.chat_message_agent, agentLabel(agent))
                    },
                    stop = if (view.working) chat::stop else null
                )
                FactsRow(
                    view.facts,
                    models,
                    { label -> modelKey(label, modelOpen) { modelOpen = it } },
                    { ChatUsageKey(view.facts, usageOpen) { usageOpen = it } }
                )
            }
        }
    )
}

/** The question takes the room that the composer and its facts leave, and the conversation keeps a strip. */
@Composable
private fun DockLayout(above: @Composable () -> Unit, card: @Composable () -> Unit, below: @Composable () -> Unit) {
    Layout(contents = listOf(above, card, below)) { (tops, cards, bottoms), constraints ->
        val free = Constraints(maxWidth = constraints.maxWidth)
        val bottom = bottoms.map { it.measure(free) }
        val top = tops.map { it.measure(free) }
        val used = bottom.sumOf { it.height } + top.sumOf { it.height }
        val room = if (constraints.hasBoundedHeight) {
            (constraints.maxHeight - used - ThreadKeeps.roundToPx()).coerceAtLeast(CardKeeps.roundToPx())
        } else {
            Constraints.Infinity
        }
        val middle = cards.map { it.measure(free.copy(maxHeight = room)) }
        val placed = top + middle + bottom
        layout(constraints.maxWidth, placed.sumOf { it.height }) {
            var y = 0
            placed.forEach {
                it.placeRelative(0, y)
                y += it.height
            }
        }
    }
}

private val ThreadKeeps = 72.dp
private val CardKeeps = 140.dp

@Composable
private fun Queued(chat: TerminalChat, queued: ChatQueued, agent: AgentKind, showTerminal: () -> Unit) {
    val colors = KodosiTheme.colors
    Row(
        Modifier.fillMaxWidth().padding(start = 48.dp, end = 4.dp, top = 2.dp, bottom = 2.dp),
        horizontalArrangement = Arrangement.End,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(Modifier.weight(1f, fill = false), horizontalAlignment = Alignment.End) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                AppIcon(AppSymbol.CLOCK, Modifier.size(12.dp), tint = colors.inkFaint)
                Spacer(Modifier.width(6.dp))
                Text(
                    queued.text,
                    style = KodosiType.footnote,
                    color = colors.ink,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false).background(
                        colors.accentSoft,
                        RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp, bottomStart = 14.dp, bottomEnd = 4.dp)
                    ).padding(horizontal = 12.dp, vertical = 8.dp)
                )
            }
            if (queued.refused != null) {
                Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    StatusSign(StatusForm.FAILED, 10.dp)
                    Spacer(Modifier.width(5.dp))
                    Text(stringResource(R.string.chat_takes_next), style = KodosiType.caption, color = colors.inkMuted)
                }
            } else if (queued.terminal) {
                FlowRow(
                    Modifier.padding(top = 2.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                    itemVerticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        stringResource(R.string.chat_shows_in_terminal, agentLabel(agent)),
                        style = KodosiType.caption,
                        color = colors.inkMuted
                    )
                    Text(
                        stringResource(R.string.chat_show_terminal),
                        style = KodosiType.caption,
                        fontWeight = FontWeight.SemiBold,
                        color = colors.accentStrong,
                        modifier = Modifier.heightIn(min = 32.dp).clickable(role = Role.Button, onClick = showTerminal)
                            .wrapContentHeight()
                    )
                }
            }
        }
        if (!queued.terminal) ActionIcon(AppSymbol.CLOSE, R.string.remove, { chat.unqueue(queued) }, face = 24.dp)
    }
}

/** The agent's list of its work while a step of it is open: how far it is and the step that it works on. */
@Composable
private fun ChatTasks(tasks: List<ChatTask>, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    var open by rememberSaveable { mutableStateOf(false) }
    val done = tasks.count { it.state == TaskState.DONE }
    val current = tasks.firstOrNull { it.state == TaskState.WORKING }
        ?: tasks.firstOrNull { it.state == TaskState.PENDING }
    val turn by animateFloatAsState(if (open) 90f else -90f, snappy(), label = "tasks chevron")
    val spoken = pluralStringResource(R.plurals.chat_tasks_done, done, done, tasks.size)
    val action = stringResource(if (open) R.string.chat_hide_tasks else R.string.chat_show_tasks)
    Column(
        modifier.fillMaxWidth().raised(colors, RoundedCornerShape(Radius.control)).padding(horizontal = 12.dp)
    ) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(
                remember { MutableInteractionSource() },
                indication = null,
                role = Role.Button,
                onClickLabel = action
            ) { open = !open }.semantics(mergeDescendants = true) { contentDescription = spoken },
            verticalAlignment = Alignment.CenterVertically
        ) {
            AppIcon(AppSymbol.TASKS, Modifier.size(12.dp), tint = colors.inkMuted)
            Spacer(Modifier.width(7.dp))
            RollingText("$done/${tasks.size}", ChatType.monoCaption, colors.inkMuted)
            Spacer(Modifier.width(8.dp))
            Box(Modifier.weight(1f)) {
                if (!open && current != null) RollingText(current.text, KodosiType.footnote, colors.ink)
            }
            Spacer(Modifier.width(6.dp))
            AppIcon(AppSymbol.FORWARD, Modifier.size(8.dp).graphicsLayer { rotationZ = turn }, tint = colors.inkFaint)
        }
        Unfolding(open) {
            Column(Modifier.padding(bottom = 10.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                tasks.forEach { task -> TaskLine(task) }
            }
        }
    }
}

@Composable
private fun TaskLine(task: ChatTask) {
    val colors = KodosiTheme.colors
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) {
            when (task.state) {
                TaskState.DONE -> StatusSign(StatusForm.DONE, 10.dp)

                TaskState.WORKING -> Canvas(Modifier.size(7.dp)) { drawCircle(colors.ink) }

                TaskState.PENDING -> Canvas(Modifier.size(8.dp)) {
                    drawCircle(colors.inkFaint, style = Stroke(1.2.dp.toPx()))
                }
            }
        }
        Spacer(Modifier.width(8.dp))
        Text(
            task.text,
            style = KodosiType.footnote,
            color = if (task.state == TaskState.WORKING) colors.ink else colors.inkMuted
        )
    }
}

@Composable
private fun FactsRow(
    facts: ChatFacts,
    models: KnownModels,
    modelKey: @Composable (String) -> Unit,
    usageKey: @Composable () -> Unit
) {
    val colors = KodosiTheme.colors
    FactsLine(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 16.dp, end = 12.dp).heightIn(min = 36.dp)
    ) {
        facts.model?.let { modelKey(models.label(it)) }
        facts.effort?.let { effort ->
            val levels = models.levels(facts.model)
            val level = levels.indexOf(effort)
            if (level >= 0 && levels.size > 1) {
                Tipped(effortLabel(effort)) { EffortBars(level, levels.size) }
            } else {
                Text(effortLabel(effort), style = KodosiType.footnote, color = colors.inkMuted, maxLines = 1)
            }
        }
        facts.mode?.let { mode ->
            val words = ChatWords.mode(mode)
            Tipped(words) { AppIcon(ChatWords.modeSymbol(mode), Modifier.size(14.dp), tint = colors.inkMuted) }
            Text(
                words,
                style = KodosiType.footnote,
                color = colors.inkMuted,
                maxLines = 1,
                modifier = Modifier.layoutId(MODE_WORDS).clearAndSetSemantics {}
            )
        }
        if (facts.context != null || facts.limits.isNotEmpty() || (facts.cost ?: 0.0) > 0) usageKey()
        if (facts.helpers > 0) {
            Count(
                AppSymbol.FRIENDS,
                facts.helpers,
                pluralStringResource(R.plurals.chat_helpers, facts.helpers, facts.helpers)
            )
        }
        if (facts.background > 0) {
            Count(
                AppSymbol.REFRESH,
                facts.background,
                pluralStringResource(R.plurals.chat_background_commands, facts.background, facts.background)
            )
        }
    }
}

private const val MODE_WORDS = "mode words"

/**
 * The facts in one line, in their order. Without room the words of the mode go first, then the
 * facts at the end of the line.
 */
@Composable
private fun FactsLine(modifier: Modifier, content: @Composable () -> Unit) {
    Layout(content, modifier) { measurables, constraints ->
        val gap = 12.dp.roundToPx()
        val near = 4.dp.roundToPx()
        val placeables = measurables.map { it.measure(Constraints()) }
        val words = measurables.indexOfFirst { it.layoutId == MODE_WORDS }
        fun before(index: Int) = if (index == words) near else gap
        fun width(shown: List<Int>) = shown.sumOf { placeables[it].width } + shown.drop(1).sumOf(::before)
        var shown = placeables.indices.toList()
        if (width(shown) > constraints.maxWidth) shown = shown - words
        while (shown.isNotEmpty() && width(shown) > constraints.maxWidth) shown = shown.dropLast(1)
        val height = maxOf(constraints.minHeight, shown.maxOfOrNull { placeables[it].height } ?: 0)
        layout(constraints.maxWidth, height) {
            var x = 0
            shown.forEachIndexed { order, index ->
                if (order > 0) x += before(index)
                val placeable = placeables[index]
                placeable.placeRelative(x, (height - placeable.height) / 2)
                x += placeable.width
            }
        }
    }
}

@Composable
private fun Count(symbol: AppSymbol, count: Int, words: String) {
    val colors = KodosiTheme.colors
    Tipped(words) {
        Row(
            Modifier.clearAndSetSemantics { contentDescription = words },
            verticalAlignment = Alignment.CenterVertically
        ) {
            AppIcon(symbol, Modifier.size(13.dp), tint = colors.inkMuted)
            Spacer(Modifier.width(3.dp))
            Text("$count", style = KodosiType.footnote, color = colors.inkMuted, maxLines = 1)
        }
    }
}

@Composable
private fun EffortBars(level: Int, count: Int) {
    val colors = KodosiTheme.colors
    val quiet = lerp(colors.surface, colors.ink, 0.2f)
    Canvas(Modifier.size(width = (count * 4 - 1.5f).dp, height = 10.dp)) {
        val bar = 2.5.dp.toPx()
        val gap = 1.5.dp.toPx()
        for (index in 0 until count) {
            val height = size.height * (0.35f + 0.65f * index / (count - 1).coerceAtLeast(1))
            drawRoundRect(
                if (index <= level) colors.accent else quiet,
                topLeft = Offset(index * (bar + gap), size.height - height),
                size = Size(bar, height),
                cornerRadius = CornerRadius(bar * 0.4f)
            )
        }
    }
}

@Composable
internal fun ContextMeter(context: ChatContext) {
    val colors = KodosiTheme.colors
    val percent = (context.used * 100).roundToInt()
    val level = contextLevel(context)
    Row(verticalAlignment = Alignment.CenterVertically) {
        UsageRing(context.used, level, Modifier.size(14.dp), stroke = 2.dp)
        Spacer(Modifier.width(4.dp))
        RollingText(
            "$percent%",
            KodosiType.footnote.copy(fontFeatureSettings = "tnum"),
            if (level == UsageLevel.NORMAL) colors.inkMuted else level.tint()
        )
    }
}

@Composable
internal fun UsageDial(limit: UsageLimit, size: Dp) {
    val colors = KodosiTheme.colors
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        UsageRing(limit.used / 100f, limit.level, Modifier.size(size))
        Text(
            "${limit.used}",
            style = ChatType.monoCaption.copy(fontSize = with(LocalDensity.current) { (size * 0.36f).toSp() }),
            color = if (limit.level == UsageLevel.NORMAL) colors.ink else limit.level.tint(),
            maxLines = 1
        )
    }
}

@Composable
internal fun UsageRing(used: Float, level: UsageLevel, modifier: Modifier = Modifier, stroke: Dp = 1.75.dp) {
    val colors = KodosiTheme.colors
    val track = lerp(colors.surface, colors.ink, 0.12f)
    val tint = if (level == UsageLevel.NORMAL) colors.inkMuted else level.tint()
    val drawn by animateFloatAsState(used.coerceIn(0f, 1f), snappy(), label = "usage")
    Canvas(modifier) {
        val line = Stroke(stroke.toPx(), cap = StrokeCap.Round)
        val inset = line.width / 2
        val box = Size(size.width - line.width, size.height - line.width)
        drawArc(track, 0f, 360f, false, Offset(inset, inset), box, style = Stroke(line.width))
        if (drawn > 0f) drawArc(tint, -90f, 360f * drawn, false, Offset(inset, inset), box, style = line)
    }
}

@Composable
private fun UsageLevel.tint() = when (this) {
    UsageLevel.NORMAL -> KodosiTheme.colors.inkMuted
    UsageLevel.NEAR -> KodosiTheme.colors.caution
    UsageLevel.REACHED -> KodosiTheme.colors.danger
}

/** A symbol names itself in a tip at a long press. */
@Composable
private fun Tipped(words: String, content: @Composable () -> Unit) {
    val colors = KodosiTheme.colors
    val haptics = LocalHapticFeedback.current
    var shown by remember { mutableStateOf(false) }
    val lift = with(LocalDensity.current) { 34.dp.roundToPx() }
    LaunchedEffect(shown) {
        if (shown) {
            delay(1600)
            shown = false
        }
    }
    Box(
        Modifier.heightIn(min = 36.dp).semantics(mergeDescendants = true) { contentDescription = words }
            .pointerInput(words) {
                detectTapGestures(onLongPress = {
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    shown = true
                })
            },
        contentAlignment = Alignment.Center
    ) {
        content()
        if (shown) {
            Popup(alignment = Alignment.TopCenter, offset = IntOffset(0, -lift)) {
                Text(
                    words,
                    style = KodosiType.caption,
                    color = colors.ink,
                    maxLines = 1,
                    modifier = Modifier.raised(colors, RoundedCornerShape(Radius.sm), elevation = Elevation.FLOATING)
                        .padding(horizontal = 10.dp, vertical = 6.dp)
                )
            }
        }
    }
}
