package com.kodosi.mobile.chat

import android.icu.text.CompactDecimalFormat
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.effortLabel
import com.kodosi.mobile.terminals.shortUsageWindow
import com.kodosi.mobile.terminals.usageWindow
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.snappy
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

/** The words in the facts' keys, a step above the facts around them. */
private val KeyWords = KodosiType.callout.copy(fontWeight = FontWeight.Medium, fontFeatureSettings = "tnum")

/** The words of the plain facts beside the keys. */
internal val FactWords = KodosiType.callout.copy(fontFeatureSettings = "tnum")

/**
 * The agent's model with its effort and its permission mode. Where the person may change them, it is
 * a key that opens the agent's part of the terminal's actions, which `/model` also opens.
 */
@Composable
internal fun ChatModelKey(
    name: String,
    effort: String?,
    levels: List<String>,
    mode: ChatMode?,
    named: Boolean,
    changes: Boolean,
    open: Boolean,
    setOpen: (Boolean) -> Unit,
    menu: (@Composable () -> Unit)?
) {
    if (!changes) {
        CompositionLocalProvider(LocalContentColor provides KodosiTheme.colors.inkMuted) {
            Row(
                Modifier.heightIn(min = 34.dp).padding(horizontal = 4.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                ModelFace(name, effort, levels, mode, named)
            }
        }
        return
    }
    Box {
        KodosiButton(
            onClick = { setOpen(true) },
            kind = if (open) ButtonKind.TINTED else ButtonKind.SECONDARY,
            size = ButtonSize.SMALL
        ) {
            ModelFace(name, effort, levels, mode, named)
            Spacer(Modifier.width(6.dp))
            KeyChevron(open)
        }
        if (menu != null) KodosiMenu(open, { setOpen(false) }) { menu() }
    }
}

@Composable
private fun ModelFace(name: String, effort: String?, levels: List<String>, mode: ChatMode?, named: Boolean) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            name,
            style = KeyWords,
            maxLines = 1,
            overflow = TextOverflow.MiddleEllipsis,
            modifier = Modifier.widthIn(max = 180.dp)
        )
        effort?.let {
            val words = effortLabel(it)
            val level = levels.indexOf(it)
            if (level >= 0 && levels.size > 1) {
                EffortBars(level, levels.size, Modifier.clearAndSetSemantics { contentDescription = words })
            } else {
                Text(words, style = KeyWords, maxLines = 1)
            }
        }
        mode?.let {
            Text(
                "·",
                style = KeyWords,
                color = KodosiTheme.colors.inkFaint,
                modifier = Modifier.clearAndSetSemantics {}
            )
            ChatModeLabel(it, named, KeyWords)
        }
    }
}

/** The permission mode of the agent: its symbol and, where there is room, its word. */
@Composable
internal fun ChatModeLabel(mode: ChatMode, named: Boolean, style: TextStyle, modifier: Modifier = Modifier) {
    val words = ChatWords.mode(mode)
    Row(
        modifier.clearAndSetSemantics { contentDescription = words },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        AppIcon(ChatWords.modeSymbol(mode), Modifier.size(15.dp), tint = LocalContentColor.current)
        if (named) Text(words, style = style, maxLines = 1)
    }
}

/** The context and the nearest limit of the agent's plan, which open all that the agent reports of its use. */
@Composable
internal fun ChatUsageKey(facts: ChatFacts, named: Boolean, open: Boolean, setOpen: (Boolean) -> Unit, menu: Boolean) {
    val colors = KodosiTheme.colors
    val words = stringResource(R.string.chat_usage)
    Box {
        KodosiButton(
            onClick = { setOpen(true) },
            modifier = Modifier.semantics { contentDescription = words },
            kind = if (open) ButtonKind.TINTED else ButtonKind.SECONDARY,
            size = ButtonSize.SMALL
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                facts.context?.let { context ->
                    val percent = (context.used * 100).roundToInt()
                    val level = contextLevel(context)
                    UsageMark(
                        context.used,
                        level,
                        if (named) stringResource(R.string.chat_context_percent, percent) else "$percent%",
                        stringResource(R.string.chat_context_used, percent)
                    )
                }
                UsageLimit.nearest(facts.limits)?.let { limit ->
                    UsageMark(
                        limit.used / 100f,
                        limit.level,
                        if (named) {
                            stringResource(
                                R.string.chat_limit_percent,
                                shortUsageWindow(limit.hours, limit.scope),
                                limit.used
                            )
                        } else {
                            "${limit.used}%"
                        },
                        stringResource(R.string.chat_usage_used, usageWindow(limit.hours, limit.scope), limit.used)
                    )
                }
                if (facts.context == null && facts.limits.isEmpty()) {
                    facts.cost?.let { Text(ChatWords.cost(it), style = KeyWords, color = colors.ink, maxLines = 1) }
                }
            }
            Spacer(Modifier.width(6.dp))
            KeyChevron(open)
        }
        if (menu) KodosiMenu(open, { setOpen(false) }) { ChatUsage(facts) }
    }
}

@Composable
private fun UsageMark(used: Float, level: UsageLevel, shown: String, spoken: String) {
    Row(
        Modifier.clearAndSetSemantics { contentDescription = spoken },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        UsageRing(used, level, Modifier.size(15.dp), stroke = 2.dp)
        Text(
            shown,
            style = KeyWords,
            color = if (level == UsageLevel.NORMAL) KodosiTheme.colors.ink else level.tint(),
            maxLines = 1
        )
    }
}

@Composable
private fun KeyChevron(open: Boolean) {
    val colors = KodosiTheme.colors
    val turn by animateFloatAsState(if (open) 180f else 0f, snappy(), label = "chevron")
    AppIcon(
        AppSymbol.CHEVRON_DOWN,
        Modifier.size(11.dp).graphicsLayer { rotationZ = turn },
        tint = if (open) colors.accentStrong else colors.inkFaint
    )
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
