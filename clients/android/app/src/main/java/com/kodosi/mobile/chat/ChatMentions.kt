package com.kodosi.mobile.chat

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.terminals.AgentKind
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.KodosiColors
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.rememberWords
import com.kodosi.mobile.ui.tint

/**
 * The agents of other terminals that a person names with `@`. The words after the draft's last `@`
 * that starts a word, on one line and of at most three words, pick the chat's peers whose names start
 * with them, then those whose names hold them.
 */
internal object ChatMentionMatch {
    private const val MOST_TYPED = 40
    private const val MOST_WORDS = 3
    private const val MOST_SHOWN = 6

    fun query(draft: String): String? {
        val at = draft.lastIndexOf('@')
        if (at < 0 || (at > 0 && draft[at - 1].inWord)) return null
        val typed = draft.substring(at + 1)
        val fits = typed.codePointCount(0, typed.length) <= MOST_TYPED &&
            typed.none { it == '\n' || it == '\r' } &&
            typed.count { it == ' ' } < MOST_WORDS
        return typed.takeIf { fits }
    }

    fun peers(peers: List<ChatPeer>, query: String): List<ChatPeer> {
        val typed = query.lowercase()
        val (starting, others) = peers.partition { it.name.lowercase().startsWith(typed) }
        return (starting + others.filter { typed in it.name.lowercase() }).take(MOST_SHOWN)
    }

    /** The draft with [peer]'s name and a space in place of the words after its last `@`. */
    fun naming(draft: String, peer: ChatPeer): String = if ('@' in draft) unnamed(draft) + "@${peer.name} " else draft

    /** The draft without its last `@` and the words after it. */
    fun unnamed(draft: String): String {
        val at = draft.lastIndexOf('@')
        return if (at < 0) draft else draft.substring(0, at)
    }

    /**
     * Where [text] names each of [mentions]: an `@` that starts a word, then the name in any case up to
     * the end of a word, the longest name where several fit.
     */
    fun named(text: String, mentions: List<ChatMention>): List<Pair<IntRange, ChatMention>> = buildList {
        val longest = mentions.filter { it.name.isNotEmpty() }.sortedByDescending { it.name.length }
        var at = text.indexOf('@')
        while (at >= 0) {
            val mention = longest.firstOrNull { names(text, at, it.name) }
            val end = at + 1 + (mention?.name?.length ?: 0)
            if (mention != null) add(at until end to mention)
            at = text.indexOf('@', end)
        }
    }

    private fun names(text: String, at: Int, name: String): Boolean {
        val end = at + 1 + name.length
        return (at == 0 || !text[at - 1].inWord) &&
            text.regionMatches(at + 1, name, 0, name.length, ignoreCase = true) &&
            (end == text.length || !text[end].inWord)
    }

    private val Char.inWord: Boolean get() = isLetterOrDigit() || this == '_'
}

/** The terminals of the chat's peers that this app knows, by their names, and what opens one by its ID. */
internal class MentionTerminals(val ids: Map<String, String>, val open: (String) -> Unit)

private val PeerRow = 48.dp

/** The peers that the person's `@` names, above the composer where the command list shows. */
@Composable
internal fun ChatPeerList(
    peers: List<ChatPeer>,
    pick: (ChatPeer) -> Unit,
    handOff: (ChatPeer) -> Unit,
    modifier: Modifier = Modifier
) {
    val colors = KodosiTheme.colors
    LazyColumn(
        modifier.fillMaxWidth().heightIn(max = PeerRow * 5.5f)
            .raised(colors, RoundedCornerShape(Radius.lg), fill = colors.raised),
        contentPadding = PaddingValues(4.dp)
    ) {
        items(peers, key = { it.terminal }) { peer -> PeerLine(peer, { pick(peer) }, { handOff(peer) }) }
    }
}

/** A peer: a tap names it in the draft, and a long press offers to name it or to hand its work over. */
@Composable
private fun PeerLine(peer: ChatPeer, pick: () -> Unit, handOff: () -> Unit) {
    val colors = KodosiTheme.colors
    val mention = stringResource(R.string.chat_mention)
    val handOffWords = stringResource(R.string.chat_hand_off)
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier.fillMaxWidth().heightIn(min = PeerRow).clip(RoundedCornerShape(Radius.sm))
                .combinedClickable(
                    onClickLabel = mention,
                    role = Role.Button,
                    onLongClick = { menu = true },
                    onClick = pick
                )
                .semantics {
                    customActions = listOf(
                        CustomAccessibilityAction(handOffWords) {
                            handOff()
                            true
                        }
                    )
                }
                .padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            AgentMark(AgentKind.of(peer.agent), 24.dp)
            Text(
                peer.name,
                style = KodosiType.body,
                fontWeight = FontWeight.Medium,
                color = colors.ink,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )
            if (peer.place.isNotEmpty()) {
                Text(
                    peer.place,
                    style = PlaceWords,
                    color = colors.inkFaint,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false)
                )
            }
        }
        KodosiMenu(menu, { menu = false }) {
            KodosiMenuItem(text = { Text(mention) }, onClick = {
                menu = false
                pick()
            })
            KodosiMenuItem(text = { Text(handOffWords) }, onClick = {
                menu = false
                handOff()
            })
        }
    }
}

private val PlaceWords = KodosiType.caption.copy(fontWeight = FontWeight.Normal)

/**
 * A person's words in a bubble of [fill], with each agent that they name as a chip of `@` and its name
 * in the agent's tint. A chip opens the agent's terminal where [terminals] has it.
 */
@Composable
internal fun MentionedWords(text: String, mentions: List<ChatMention>, fill: Color, terminals: MentionTerminals) {
    val colors = KodosiTheme.colors
    val words = rememberWords(text, true)
    val chipped = remember(words, mentions, fill, colors, terminals) {
        mentionChips(words, mentions, fill, colors, terminals)
    }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    Text(
        chipped.words,
        style = KodosiType.body,
        color = colors.ink,
        onTextLayout = { layout = it },
        modifier = Modifier.drawBehind { layout?.let { shown -> chipped.chips.forEach { drawChip(shown, it) } } }
    )
}

private class Chip(val start: Int, val end: Int, val fill: Color)

private class MentionChips(val words: AnnotatedString, val chips: List<Chip>)

private fun mentionChips(
    words: AnnotatedString,
    mentions: List<ChatMention>,
    fill: Color,
    colors: KodosiColors,
    terminals: MentionTerminals
): MentionChips {
    val chips = mutableListOf<Chip>()
    val shown = buildAnnotatedString {
        var from = 0
        for ((range, mention) in ChatMentionMatch.named(words.text, mentions)) {
            append(words.subSequence(from, range.first))
            val start = length
            val kind = AgentKind.of(mention.agent)
            val ink = if (kind.isAgent) lerp(kind.tint(), colors.ink, 0.2f) else colors.inkMuted
            val style = SpanStyle(color = ink, fontWeight = FontWeight.SemiBold)
            val id = terminals.ids[mention.name]
            if (id == null) {
                withStyle(style) { append("@${mention.name}") }
            } else {
                withLink(LinkAnnotation.Clickable(id, TextLinkStyles(style)) { terminals.open(id) }) {
                    append("@${mention.name}")
                }
            }
            chips += Chip(start, length, lerp(fill, ink, 0.18f))
            from = range.last + 1
        }
        append(words.subSequence(from, words.length))
    }
    return MentionChips(shown, chips)
}

private fun DrawScope.drawChip(layout: TextLayoutResult, chip: Chip) {
    val reach = 3.dp.toPx()
    val inset = 1.dp.toPx()
    val radius = CornerRadius(5.dp.toPx())
    val first = layout.getLineForOffset(chip.start)
    val last = layout.getLineForOffset(chip.end - 1)
    for (line in first..last) {
        val left = if (line == first) layout.getBoundingBox(chip.start).left else layout.getLineLeft(line)
        val right = if (line == last) layout.getBoundingBox(chip.end - 1).right else layout.getLineRight(line)
        val top = layout.getLineTop(line) + inset
        val bottom = layout.getLineBottom(line) - inset
        drawRoundRect(chip.fill, Offset(left - reach, top), Size(right - left + 2 * reach, bottom - top), radius)
    }
}

/** The line of a person's message that handed [handoff]'s work to this agent. */
@Composable
internal fun HandOffLine(handoff: ChatMention) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        AgentMark(AgentKind.of(handoff.agent), 18.dp)
        Spacer(Modifier.width(8.dp))
        Text(handOffWords(handoff.name), style = KodosiType.body, color = KodosiTheme.colors.ink)
    }
}

/** Hand-off from the agent of the terminal [name], with the name in semibold. */
@Composable
internal fun handOffWords(name: String): AnnotatedString {
    val words = stringResource(R.string.chat_hand_off_from, NAME_SLOT)
    return remember(words, name) {
        val at = words.indexOf(NAME_SLOT)
        buildAnnotatedString {
            if (at < 0) {
                append(words)
            } else {
                append(words.substring(0, at))
                withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(name) }
                append(words.substring(at + NAME_SLOT.length))
            }
        }
    }
}

// Holds the place of the name in the translated words, so that only the name takes the weight.
private const val NAME_SLOT = "￼"

/** The person's message as its bubble shows it: the line of a hand-off, then the words. */
@Composable
internal fun PersonSaid(person: ChatRow.Kind.Person, fill: Color, terminals: MentionTerminals) {
    val handoff = person.handoff
    if (handoff == null) {
        MentionedWords(person.text, person.mentions, fill, terminals)
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        HandOffLine(handoff)
        if (person.text.isNotEmpty()) MentionedWords(person.text, person.mentions, fill, terminals)
    }
}
