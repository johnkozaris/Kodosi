package com.kodosi.mobile.chat

import android.icu.text.CompactDecimalFormat
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.usageWindow
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.raised
import kotlin.math.ceil
import kotlin.math.roundToInt

/** The agent's slash commands that start as a person types after a slash, then the ones that hold the typed letters. */
internal object ChatCommandMatch {
    fun commands(commands: List<ChatCommand>, draft: String): List<ChatCommand> {
        if (!draft.startsWith("/") || draft.any { it.isWhitespace() }) return emptyList()
        val typed = draft.drop(1).lowercase()
        val starting = commands.filter { it.name.lowercase().startsWith(typed) }
        if (typed.isEmpty()) return starting
        return starting + commands.filter { !it.name.lowercase().startsWith(typed) && typed in it.name.lowercase() }
    }
}

private val CommandRow = 40.dp

/** The commands that fit the draft, above the composer, as the agent's own menu shows them in its terminal. */
@Composable
internal fun ChatCommandList(
    commands: List<ChatCommand>,
    choose: (ChatCommand) -> Unit,
    modifier: Modifier = Modifier
) {
    val colors = KodosiTheme.colors
    val label = stringResource(R.string.chat_commands)
    LazyColumn(
        modifier.fillMaxWidth().heightIn(max = CommandRow * 6.5f)
            .raised(colors, RoundedCornerShape(Radius.lg), fill = colors.raised)
            .semantics { contentDescription = label },
        contentPadding = PaddingValues(4.dp)
    ) {
        itemsIndexed(commands, key = { _, command -> command.name }) { _, command ->
            Row(
                Modifier.fillMaxWidth().heightIn(min = CommandRow).clip(RoundedCornerShape(Radius.sm))
                    .clickable(role = Role.Button) { choose(command) }.padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                Text("/${command.name}", style = ChatType.mono, color = colors.ink, maxLines = 1, softWrap = false)
                Text(
                    command.description,
                    style = KodosiType.footnote,
                    color = colors.inkMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )
            }
        }
    }
}

/** The context and the nearest limit of the agent's plan, which open all that the agent reports of its use. */
@Composable
internal fun ChatUsageKey(facts: ChatFacts, open: Boolean, setOpen: (Boolean) -> Unit) {
    val colors = KodosiTheme.colors
    val words = stringResource(R.string.chat_usage)
    Box {
        Row(
            Modifier.heightIn(min = 36.dp).clip(RoundedCornerShape(Radius.sm))
                .background(if (open) colors.ink.copy(alpha = 0.07f) else Color.Transparent)
                .clickable(remember { MutableInteractionSource() }, null, role = Role.Button, onClickLabel = words) {
                    setOpen(!open)
                }
                .clearAndSetSemantics { contentDescription = words }
                .padding(horizontal = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            facts.context?.let { ContextMeter(it) }
            UsageLimit.nearest(facts.limits)?.let { UsageDial(it, 20.dp) }
            if (facts.context == null && facts.limits.isEmpty()) {
                facts.cost?.let {
                    Text(ChatWords.cost(it), style = KodosiType.footnote, color = colors.inkMuted, maxLines = 1)
                }
            }
        }
        KodosiMenu(open, { setOpen(false) }) { ChatUsage(facts) }
    }
}

/** What the agent reports of its use: the limits of its plan with their resets, its context and the cost of the conversation. */
@Composable
private fun ChatUsage(facts: ChatFacts) {
    val colors = KodosiTheme.colors
    Column(
        Modifier.widthIn(min = 240.dp).padding(horizontal = 14.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        facts.context?.let { context ->
            val percent = (context.used * 100).roundToInt()
            UsageLine(
                { UsageRing(context.used, contextLevel(context), Modifier.size(20.dp), stroke = 2.5.dp) },
                stringResource(R.string.chat_context_used, percent),
                stringResource(
                    R.string.chat_tokens_of,
                    ChatWords.tokens(context.tokens),
                    ChatWords.tokens(context.window)
                )
            )
        }
        facts.limits.forEach { limit ->
            UsageLine(
                { UsageDial(limit, 22.dp) },
                stringResource(R.string.chat_usage_used, usageWindow(limit.hours, limit.scope), limit.used),
                ChatWords.resetIn(limit)?.let { stringResource(R.string.chat_resets_in, it) }
            )
        }
        facts.cost?.takeIf { it > 0 }?.let { cost ->
            UsageLine(
                {
                    Box(
                        Modifier.size(20.dp).border(1.5.dp, colors.inkMuted, CircleShape),
                        contentAlignment = Alignment.Center
                    ) {
                        Text("$", style = KodosiType.caption, color = colors.inkMuted)
                    }
                },
                stringResource(R.string.chat_this_conversation),
                ChatWords.cost(cost)
            )
        }
    }
}

@Composable
private fun UsageLine(sign: @Composable () -> Unit, title: String, detail: String?) {
    val colors = KodosiTheme.colors
    Row(
        Modifier.semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) { sign() }
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = KodosiType.footnote, color = colors.ink)
            detail?.let { Text(it, style = KodosiType.caption, color = colors.inkMuted) }
        }
    }
}

internal fun contextLevel(context: ChatContext): UsageLevel = when {
    context.used > 0.85f -> UsageLevel.REACHED
    context.used >= 0.65f -> UsageLevel.NEAR
    else -> UsageLevel.NORMAL
}

/** How long until a limit of a plan starts again, in words, rounded up to its two largest units. */
@Composable
internal fun ChatWords.resetIn(limit: UsageLimit): String? {
    val resets = limit.resetsAtMs ?: return null
    val locale = LocalConfiguration.current.locales[0]
    val left = (resets - System.currentTimeMillis()) / 1000.0
    if (left <= 0) return null
    return remember(resets / 60_000, locale) {
        val minutes = maxOf(1.0, ceil(left / 60)).toLong()
        val parts = when {
            minutes >= 24 * 60 -> {
                val hours = ceil(left / 3600).toLong()
                listOf(Measure(hours / 24, MeasureUnit.DAY), Measure(hours % 24, MeasureUnit.HOUR))
            }

            minutes >= 60 -> listOf(Measure(minutes / 60, MeasureUnit.HOUR), Measure(minutes % 60, MeasureUnit.MINUTE))

            else -> listOf(Measure(minutes, MeasureUnit.MINUTE))
        }.filter { it.number.toLong() > 0 }
        MeasureFormat.getInstance(locale, MeasureFormat.FormatWidth.WIDE).formatMeasures(*parts.toTypedArray())
    }
}

/** A count of tokens in its short form, such as 45K. */
@Composable
internal fun ChatWords.tokens(count: Long): String {
    val locale = LocalConfiguration.current.locales[0]
    return remember(count, locale) {
        CompactDecimalFormat.getInstance(locale, CompactDecimalFormat.CompactStyle.SHORT).format(count)
    }
}
