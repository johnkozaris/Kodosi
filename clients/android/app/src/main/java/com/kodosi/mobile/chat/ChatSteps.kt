package com.kodosi.mobile.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.StatusForm
import com.kodosi.mobile.terminals.lastName
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.rememberWords
import com.kodosi.mobile.ui.snappy

private const val SHOWN_CHANGE_LINES = 40

/**
 * The work of a turn as one quiet line: what the steps did and the one sign of how they ended.
 * The line is there from the first step, so a person can open a step while it runs.
 */
@Composable
internal fun ChatWork(items: List<ChatItem>, hold: () -> Unit) {
    val colors = KodosiTheme.colors
    var open by rememberSaveable(items.first().id) { mutableStateOf(false) }
    val tools = items.tools
    val ended = tools.filter { !it.state.isOpen }
    val did = ended.filter { it.state == ChatToolState.DONE || it.state == ChatToolState.FAILED }
    val summary = did.takeIf { it.isNotEmpty() }?.let { ChatWords.summary(it) }
    val words = (listOfNotNull(summary) + ChatWords.setbacks(ended))
        .ifEmpty { listOf(pluralStringResource(R.plurals.chat_steps, tools.size, tools.size)) }
        .joinToString(" · ")
    val sign = when {
        tools.any { it.state.isOpen } -> null
        tools.any { it.state == ChatToolState.FAILED } -> StatusForm.FAILED
        tools.all { it.state == ChatToolState.DONE } -> StatusForm.DONE
        else -> null
    }
    val turn by animateFloatAsState(if (open) 90f else 0f, snappy(), label = "steps chevron")
    val action = stringResource(if (open) R.string.chat_hide_steps else R.string.chat_show_steps)
    Column {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 30.dp).clickable(
                remember { MutableInteractionSource() },
                indication = null,
                role = Role.Button,
                onClickLabel = action
            ) {
                hold()
                open = !open
            },
            verticalAlignment = Alignment.CenterVertically
        ) {
            ChatWords.main(tools)?.let {
                AppIcon(ChatWords.symbol(it), Modifier.size(12.dp), tint = colors.inkFaint)
                Spacer(Modifier.width(7.dp))
            }
            RollingText(
                words,
                KodosiType.caption,
                colors.inkMuted,
                Modifier.weight(1f, fill = false).padding(vertical = 4.dp),
                maxLines = 2
            )
            Spacer(Modifier.width(6.dp))
            PoppingSign(sign, 11.dp)
            Spacer(Modifier.width(6.dp))
            AppIcon(
                AppSymbol.FORWARD,
                Modifier.size(8.dp).graphicsLayer { rotationZ = turn },
                tint = colors.inkFaint
            )
        }
        Unfolding(open) {
            val shown = remember { items.map { it.id }.toSet() }
            val fading = fade<Float>()
            val sizing = fade<IntSize>()
            Column(Modifier.padding(start = 2.dp, bottom = 4.dp)) {
                items.forEach { item ->
                    key(item.id) {
                        AnimatedVisibility(
                            remember { MutableTransitionState(item.id in shown).apply { targetState = true } },
                            enter = fadeIn(fading) + expandVertically(sizing)
                        ) { ChatStepLine(item, hold) }
                    }
                }
            }
        }
    }
}

@Composable
private fun ChatStepLine(item: ChatItem, hold: () -> Unit) {
    val colors = KodosiTheme.colors
    when (val body = item.body) {
        is ChatBody.Tool -> {
            val tool = body.tool
            val details = shows(tool)
            var open by rememberSaveable(item.id) { mutableStateOf(false) }
            val turn by animateFloatAsState(if (open) 90f else 0f, snappy(), label = "step chevron")
            Column {
                Row(
                    Modifier.fillMaxWidth().height(30.dp).then(
                        if (details) {
                            Modifier.clickable(
                                remember { MutableInteractionSource() },
                                indication = null,
                                role = Role.Button
                            ) {
                                hold()
                                open = !open
                            }
                        } else {
                            Modifier
                        }
                    ),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    AppIcon(ChatWords.symbol(tool.kind), Modifier.size(12.dp), tint = colors.inkFaint)
                    Spacer(Modifier.width(7.dp))
                    Text(
                        ChatWords.step(tool),
                        style = if (ChatWords.monospaced(tool.kind)) ChatType.monoCaption else KodosiType.caption,
                        color = if (tool.state.isOpen) colors.ink else colors.inkMuted,
                        maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                        modifier = Modifier.weight(1f, fill = false)
                    )
                    Spacer(Modifier.width(6.dp))
                    StepState(tool.state)
                    if (details) {
                        Spacer(Modifier.width(6.dp))
                        AppIcon(
                            AppSymbol.FORWARD,
                            Modifier.size(8.dp).graphicsLayer { rotationZ = turn },
                            tint = colors.inkFaint
                        )
                    }
                }
                Unfolding(open) { StepDetail(tool, Modifier.padding(start = 19.dp, bottom = 8.dp)) }
            }
        }

        is ChatBody.Text -> Text(
            rememberWords(body.text, true),
            style = KodosiType.footnote,
            color = colors.inkMuted,
            modifier = Modifier.padding(vertical = 4.dp)
        )

        else -> Unit
    }
}

@Composable
private fun StepState(state: ChatToolState) {
    val colors = KodosiTheme.colors
    when (state) {
        ChatToolState.DENIED ->
            Text(stringResource(R.string.chat_denied), style = KodosiType.caption, color = colors.inkFaint)

        ChatToolState.STOPPED ->
            Text(stringResource(R.string.chat_stopped_step), style = KodosiType.caption, color = colors.inkFaint)

        else -> PoppingSign(
            when (state) {
                ChatToolState.DONE -> StatusForm.DONE
                ChatToolState.FAILED -> StatusForm.FAILED
                else -> null
            },
            11.dp
        )
    }
}

private fun shows(tool: ChatTool): Boolean = when (val detail = tool.detail) {
    is ChatToolDetail.Command -> detail.output.isNotEmpty() || detail.background || detail.command != tool.target
    is ChatToolDetail.Change, is ChatToolDetail.Plan, is ChatToolDetail.Agent -> true
    is ChatToolDetail.Output -> detail.text.isNotEmpty()
    is ChatToolDetail.Question -> detail.questions.isNotEmpty()
    null -> false
}

@Composable
private fun StepDetail(tool: ChatTool, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        when (val detail = tool.detail) {
            is ChatToolDetail.Command -> {
                if (detail.command != tool.target || detail.output.isNotEmpty()) Printed(detail.command, detail.output)
                if (detail.background) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        AppIcon(AppSymbol.REFRESH, Modifier.size(10.dp), tint = colors.inkFaint)
                        Spacer(Modifier.width(5.dp))
                        Text(
                            stringResource(R.string.chat_in_background),
                            style = KodosiType.caption,
                            color = colors.inkFaint
                        )
                    }
                }
            }

            is ChatToolDetail.Change -> {
                Row(
                    Modifier.semantics(mergeDescendants = true) { contentDescription = detail.path },
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Text(
                        lastName(detail.path),
                        style = ChatType.monoCaption,
                        color = colors.inkMuted,
                        maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                        modifier = Modifier.weight(1f, fill = false)
                    )
                    Text("+${detail.added}", style = ChatType.monoCaption, color = colors.ready)
                    Text("−${detail.removed}", style = ChatType.monoCaption, color = colors.danger)
                }
                ChangeLines(detail.lines)
            }

            is ChatToolDetail.Output -> Printed("", detail.text)

            is ChatToolDetail.Question -> SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    detail.questions.forEachIndexed { index, question ->
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(question.question, style = KodosiType.footnote, color = colors.inkMuted)
                            detail.answers.getOrNull(index)?.let {
                                Text(
                                    it,
                                    style = KodosiType.footnote.copy(fontWeight = FontWeight.SemiBold),
                                    color = colors.ink
                                )
                            }
                        }
                    }
                }
            }

            is ChatToolDetail.Plan -> ChatProse(detail.text, live = false)

            is ChatToolDetail.Agent -> {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (detail.name.isNotEmpty()) Text(detail.name, style = ChatType.monoCaption, color = colors.ink)
                    Text(
                        pluralStringResource(R.plurals.chat_steps, detail.steps, detail.steps),
                        style = ChatType.monoCaption,
                        color = colors.inkFaint
                    )
                }
                detail.doing?.let { step ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        AppIcon(ChatWords.symbol(step.kind), Modifier.size(11.dp), tint = colors.inkFaint)
                        Spacer(Modifier.width(6.dp))
                        Text(
                            step.target,
                            style = ChatType.monoCaption,
                            color = colors.inkMuted,
                            maxLines = 1,
                            overflow = TextOverflow.MiddleEllipsis
                        )
                    }
                }
                if (detail.said.isNotEmpty()) {
                    Text(
                        rememberWords(detail.said, true),
                        style = KodosiType.footnote,
                        color = colors.inkMuted,
                        maxLines = 6,
                        overflow = TextOverflow.Ellipsis
                    )
                }
            }

            null -> Unit
        }
    }
}

/** Code that a step ran and what it printed. What a command printed starts at its end, where a result or a fault is. */
@Composable
private fun Printed(text: String, output: String) {
    val colors = KodosiTheme.colors
    val scroll = rememberScrollState()
    CodeWell(Modifier.fillMaxWidth()) {
        Box(Modifier.heightIn(max = 180.dp).verticalScroll(scroll, reverseScrolling = output.isNotEmpty())) {
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (text.isNotEmpty()) Text(text, style = ChatType.mono, color = colors.ink)
                    if (output.isNotEmpty()) Text(output, style = ChatType.mono, color = colors.inkMuted)
                }
            }
        }
    }
}

@Composable
internal fun ChangeLines(lines: List<ChangeLine>, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    CodeWell(modifier.fillMaxWidth()) {
        SelectionContainer {
            Column {
                lines.take(SHOWN_CHANGE_LINES).forEach { line ->
                    val color = when (line.sign) {
                        ChangeSign.ADDED -> colors.ready
                        ChangeSign.REMOVED -> colors.danger
                        ChangeSign.SAME -> colors.inkFaint
                    }
                    Row {
                        Text(
                            when (line.sign) {
                                ChangeSign.ADDED -> "+"
                                ChangeSign.REMOVED -> "−"
                                ChangeSign.SAME -> " "
                            },
                            style = ChatType.mono,
                            color = color
                        )
                        Spacer(Modifier.width(6.dp))
                        Text(
                            line.text,
                            style = ChatType.mono,
                            color = color,
                            maxLines = 4,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                }
                if (lines.size > SHOWN_CHANGE_LINES) Text("…", style = ChatType.mono, color = colors.inkFaint)
            }
        }
    }
}
