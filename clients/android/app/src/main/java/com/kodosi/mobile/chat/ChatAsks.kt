package com.kodosi.mobile.chat

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateBounds
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.movableContentOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.LookaheadScope
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.reducedMotion
import com.kodosi.mobile.ui.snappy
import com.kodosi.mobile.ui.well

/**
 * A question of the agent, open above the composer. The key that a person pressed stays down until
 * the question is gone, and the others wait. Above a phone's keyboard the card gives up the text of
 * a change, a command or a plan first, and keeps its keys.
 */
@Composable
internal fun ChatAskCard(chat: TerminalChat, view: ChatView, ask: ChatAsk, agent: AgentKind) {
    val colors = KodosiTheme.colors
    val body = ask.body
    Box(
        Modifier.fillMaxWidth()
            .raised(colors, RoundedCornerShape(Radius.lg), fill = colors.raised, elevation = Elevation.LIFTED)
            .padding(horizontal = 12.dp, vertical = 11.dp)
    ) {
        when (body) {
            is AskBody.Permission -> {
                val tool = body.tool
                val whole = whole(tool)
                AskLayout(
                    header = {
                        AskHeader(agent, ChatWords.symbol(tool.kind), cue = ask.id, helper = body.helper) {
                            Text(
                                tool.target.ifEmpty { tool.name },
                                style = ChatType.mono,
                                color = colors.ink,
                                maxLines = if (whole == null) 3 else 1,
                                overflow = if (whole == null) TextOverflow.Ellipsis else TextOverflow.MiddleEllipsis
                            )
                        }
                    },
                    body = { PermissionBody(tool, whole) },
                    keys = { PermissionKeys(chat, view, ask, body.always) },
                    keepsBody = KeepsBody
                )
            }

            is AskBody.Plan -> AskLayout(
                header = {
                    AskHeader(agent, ChatWords.symbol(ChatToolKind.PLAN), cue = ask.id) {
                        Text(stringResource(R.string.chat_mode_plan), style = KodosiType.subhead, color = colors.ink)
                    }
                },
                body = {
                    if (body.text.isNotEmpty()) {
                        Box(Modifier.verticalScroll(rememberScrollState())) { ChatProse(body.text, live = false) }
                    }
                },
                keys = { PlanKeys(chat, view, ask, body.choices) },
                keepsBody = KeepsBody
            )

            is AskBody.Question -> Questions(chat, view, ask, body.questions, agent)
        }
    }
}

private val KeepsBody = 48.dp
private val BodyMost = 320.dp

/** A command that its line does not hold whole shows whole below it: a person reads all of what they allow. */
private fun whole(tool: ChatTool): String? =
    (tool.detail as? ChatToolDetail.Command)?.command?.takeIf { it != tool.target || it.length > 96 }

/** A question without [symbol] ends with the question sign, and an approval with the hand that waves when it comes. */
@Composable
private fun AskHeader(
    agent: AgentKind,
    symbol: AppSymbol?,
    cue: String,
    helper: String = "",
    title: @Composable () -> Unit
) {
    val colors = KodosiTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AgentMark(agent, 20.dp)
            Spacer(Modifier.width(8.dp))
            symbol?.let {
                AppIcon(it, Modifier.size(12.dp), tint = colors.inkFaint)
                Spacer(Modifier.width(7.dp))
            }
            Box(Modifier.weight(1f)) { title() }
            Spacer(Modifier.width(8.dp))
            if (symbol != null) WavingHand(cue, 14.dp) else PoppingSign(StatusForm.QUESTION, 14.dp)
        }
        if (helper.isNotEmpty()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                AppIcon(AppSymbol.FRIENDS, Modifier.size(11.dp), tint = colors.inkFaint)
                Spacer(Modifier.width(6.dp))
                Text(helper, style = KodosiType.caption, color = colors.inkFaint, maxLines = 1)
            }
        }
    }
}

@Composable
private fun PermissionBody(tool: ChatTool, whole: String?) {
    val change = tool.detail as? ChatToolDetail.Change
    Column(Modifier.verticalScroll(rememberScrollState())) {
        if (whole != null) {
            CodeWell(Modifier.fillMaxWidth()) {
                SelectionContainer { Text(whole, style = ChatType.mono, color = KodosiTheme.colors.ink) }
            }
        } else if (change != null && change.lines.isNotEmpty()) {
            ChangeLines(change.lines)
        }
    }
}

@Composable
private fun PermissionKeys(chat: TerminalChat, view: ChatView, ask: ChatAsk, always: AskAlways?) {
    val colors = KodosiTheme.colors
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        AnswerKey(
            chat,
            view,
            ask,
            ChatAnswer.Allow,
            ButtonKind.PRIMARY,
            AnnotatedString(stringResource(R.string.chat_allow))
        )
        AnswerKey(
            chat,
            view,
            ask,
            ChatAnswer.Deny(null),
            ButtonKind.SECONDARY,
            AnnotatedString(stringResource(R.string.chat_deny))
        )
        val (offer, monospaced) = when (always) {
            is AskAlways.Rule -> always.text to true
            is AskAlways.Mode -> ChatWords.mode(always.mode) to false
            is AskAlways.Folder -> always.name to true
            null -> null to false
        }
        if (offer != null) {
            val words = stringResource(R.string.chat_always, offer)
            val label = remember(words, offer, monospaced, colors.inkMuted) {
                buildAnnotatedString {
                    append(words)
                    val start = words.indexOf(offer)
                    if (monospaced && start >= 0) {
                        addStyle(
                            SpanStyle(fontFamily = ChatType.mono.fontFamily, color = colors.inkMuted),
                            start,
                            start + offer.length
                        )
                    }
                }
            }
            AnswerKey(chat, view, ask, ChatAnswer.Always, ButtonKind.SECONDARY, label)
        }
    }
}

@Composable
private fun PlanKeys(chat: TerminalChat, view: ChatView, ask: ChatAsk, choices: List<PlanChoice>) {
    Keys(stacked = LocalConfiguration.current.smallestScreenWidthDp < 600) { modifier ->
        choices.forEachIndexed { index, choice ->
            AnswerKey(
                chat,
                view,
                ask,
                ChatAnswer.Plan(choice),
                if (index == 0) ButtonKind.PRIMARY else ButtonKind.SECONDARY,
                AnnotatedString(planLabel(choice)),
                modifier
            )
        }
        AnswerKey(
            chat,
            view,
            ask,
            ChatAnswer.Deny(null),
            ButtonKind.SECONDARY,
            AnnotatedString(stringResource(R.string.chat_keep_planning)),
            modifier
        )
    }
}

/** On a phone the ways on after a plan stand under each other at the full width of the card. */
@Composable
private fun Keys(stacked: Boolean, content: @Composable (Modifier) -> Unit) {
    if (stacked) {
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            content(Modifier.fillMaxWidth())
        }
    } else {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            content(Modifier)
        }
    }
}

@Composable
private fun planLabel(choice: PlanChoice): String = stringResource(
    when (choice) {
        PlanChoice.GO -> R.string.approve
        PlanChoice.ASK -> R.string.chat_approve_ask
        PlanChoice.EDITS -> R.string.chat_approve_edits
        PlanChoice.AUTO -> R.string.chat_approve_auto
        PlanChoice.ALONE -> R.string.chat_approve_alone
        PlanChoice.HELPERS -> R.string.chat_approve_helpers
        PlanChoice.LEAVE -> R.string.chat_leave_plan
    }
)

@Composable
private fun AnswerKey(
    chat: TerminalChat,
    view: ChatView,
    ask: ChatAsk,
    answer: ChatAnswer,
    kind: ButtonKind,
    label: AnnotatedString,
    modifier: Modifier = Modifier
) {
    val haptics = LocalHapticFeedback.current
    val given = view.answers[ask.id]
    KodosiButton(
        onClick = {
            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
            chat.answer(ask, answer)
        },
        modifier = modifier,
        kind = kind,
        enabled = given == null || given == answer,
        down = given == answer
    ) {
        Text(label, maxLines = 2, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.Center)
    }
}

/** One question with one choice goes at the first press; more of them wait for Send. */
@Composable
private fun Questions(
    chat: TerminalChat,
    view: ChatView,
    ask: ChatAsk,
    questions: List<ChatQuestion>,
    agent: AgentKind
) {
    val colors = KodosiTheme.colors
    val haptics = LocalHapticFeedback.current
    var picked by remember(ask.id) { mutableStateOf(mapOf<Int, List<String>>()) }
    val direct = questions.size == 1 && !questions[0].multiple
    val sent = (view.answers[ask.id] as? ChatAnswer.Answers)?.answers
    val headers = questions.map { it.header }.filter { it.isNotEmpty() }.joinToString(" · ")
    val title = headers.ifEmpty { questions.firstOrNull()?.question.orEmpty() }
    AskLayout(
        header = {
            AskHeader(agent, symbol = null, cue = ask.id) {
                Text(
                    title,
                    style = KodosiType.subhead,
                    color = colors.ink,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
            }
        },
        body = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                questions.forEachIndexed { index, question ->
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (headers.isNotEmpty() || index > 0) {
                            SelectionContainer {
                                Text(question.question, style = KodosiType.callout, color = colors.ink)
                            }
                        }
                        Options(question, sent?.getOrNull(index) ?: picked[index].orEmpty(), sent == null, direct) {
                            if (direct) {
                                haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                                chat.answer(ask, ChatAnswer.Answers(listOf(listOf(it.label))))
                            } else {
                                val now = picked[index].orEmpty()
                                val next = when {
                                    !question.multiple -> listOf(it.label)
                                    it.label in now -> now - it.label
                                    else -> now + it.label
                                }
                                picked = picked + (index to next)
                            }
                        }
                    }
                }
            }
        },
        keys = {
            if (!direct) {
                KodosiButton(
                    onClick = {
                        haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                        chat.answer(ask, ChatAnswer.Answers(questions.indices.map { picked[it].orEmpty() }))
                    },
                    enabled = sent == null && questions.indices.all { picked[it].orEmpty().isNotEmpty() },
                    down = sent != null
                ) { Text(stringResource(R.string.send)) }
            }
        },
        keepsBody = 0.dp
    )
}

/**
 * The options of one question sink into a well as the choices of a stacked segmented control do.
 * A chosen row rises as the raised thumb: with one answer the one thumb slides to it, and with many
 * answers each chosen row has its own and ends with a copper check.
 */
@Composable
private fun Options(
    question: ChatQuestion,
    chosen: List<String>,
    open: Boolean,
    direct: Boolean,
    choose: (ChatOption) -> Unit
) {
    val colors = KodosiTheme.colors
    val reduced = reducedMotion()
    val moving = snappy<Rect>()
    val thumbShape = RoundedCornerShape(7.dp)
    LookaheadScope {
        val scope = this
        val thumb = remember(colors) {
            movableContentOf<Modifier> { modifier -> Box(modifier.raised(colors, thumbShape, fill = colors.raised)) }
        }
        Column(
            Modifier.fillMaxWidth().well(colors, RoundedCornerShape(10.dp)).padding(3.dp)
                .then(if (question.multiple) Modifier else Modifier.selectableGroup()),
            verticalArrangement = Arrangement.spacedBy(2.dp)
        ) {
            question.options.forEach { option ->
                val on = option.label in chosen
                val interaction = remember { MutableInteractionSource() }
                val pressed by interaction.collectIsPressedAsState()
                val scale by animateFloatAsState(if (pressed) 0.97f else 1f, snappy(), label = "option press")
                Box(
                    Modifier.fillMaxWidth().graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                    }.then(
                        if (question.multiple) {
                            Modifier.toggleable(on, interaction, null, enabled = open, role = Role.Checkbox) {
                                choose(option)
                            }
                        } else {
                            Modifier.selectable(
                                on,
                                interaction,
                                null,
                                enabled = open,
                                role = if (direct) Role.Button else Role.RadioButton
                            ) { choose(option) }
                        }
                    )
                ) {
                    if (on && question.multiple) {
                        Box(Modifier.matchParentSize().raised(colors, thumbShape, fill = colors.raised))
                    } else if (on) {
                        thumb(
                            if (reduced) {
                                Modifier.matchParentSize()
                            } else {
                                Modifier.matchParentSize().animateBounds(scope, boundsTransform = { _, _ -> moving })
                            }
                        )
                    }
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 40.dp).padding(horizontal = 10.dp, vertical = 7.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                            Text(
                                option.label,
                                style = KodosiType.subhead,
                                color = if (on) colors.ink else colors.inkMuted
                            )
                            if (option.description.isNotEmpty()) {
                                Text(
                                    option.description,
                                    style = KodosiType.footnote,
                                    color = colors.inkFaint,
                                    maxLines = 3,
                                    overflow = TextOverflow.Ellipsis
                                )
                            }
                        }
                        if (question.multiple) {
                            Spacer(Modifier.width(10.dp))
                            CheckCircle(on)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CheckCircle(on: Boolean) {
    val colors = KodosiTheme.colors
    val fading = fade<Float>()
    val popping = snappy<Float>()
    AnimatedContent(
        on,
        transitionSpec = {
            (fadeIn(fading) + scaleIn(popping, initialScale = 0.6f)) togetherWith
                (fadeOut(fading) + scaleOut(popping, targetScale = 0.6f))
        },
        label = "check"
    ) { checked ->
        if (checked) {
            Box(Modifier.size(18.dp).background(colors.accent, CircleShape), contentAlignment = Alignment.Center) {
                AppIcon(AppSymbol.CHECK, Modifier.size(10.dp), tint = colors.onAccent)
            }
        } else {
            Box(Modifier.size(18.dp).border(1.5.dp, colors.inkFaint, CircleShape))
        }
    }
}

@Composable
private fun AskLayout(
    header: @Composable () -> Unit,
    body: @Composable () -> Unit,
    keys: @Composable () -> Unit,
    keepsBody: Dp
) {
    Layout(contents = listOf(header, body, keys)) { (headers, bodies, keyRows), constraints ->
        val loose = constraints.copy(minWidth = 0, minHeight = 0, maxHeight = Constraints.Infinity)
        val gap = 10.dp.roundToPx()
        val top = headers.map { it.measure(loose) }
        val bottom = keyRows.map { it.measure(loose) }
        val headerHeight = top.sumOf { it.height }
        val keysHeight = bottom.sumOf { it.height }
        val room = if (constraints.hasBoundedHeight) {
            constraints.maxHeight - headerHeight - keysHeight - gap * 2
        } else {
            BodyMost.roundToPx()
        }
        val middle = if (room < keepsBody.roundToPx()) {
            emptyList()
        } else {
            bodies.map { it.measure(loose.copy(maxHeight = room.coerceAtLeast(0))) }
        }
        val parts = listOf(top, middle, bottom).filter { part -> part.sumOf { it.height } > 0 }
        val height = parts.sumOf { part -> part.sumOf { it.height } } + gap * (parts.size - 1).coerceAtLeast(0)
        layout(constraints.maxWidth, height) {
            var y = 0
            parts.forEach { part ->
                part.forEach {
                    it.placeRelative(0, y)
                    y += it.height
                }
                y += gap
            }
        }
    }
}
