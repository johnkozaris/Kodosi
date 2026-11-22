package com.kodosi.mobile.chat

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.rememberWords

internal sealed interface ProseBlock {
    data class Lines(val text: String) : ProseBlock

    data class Heading(val text: String) : ProseBlock

    data class Code(val text: String) : ProseBlock

    data class Table(val text: String) : ProseBlock

    companion object {
        /** A fence that is still open while the agent writes holds the rest. */
        fun blocks(text: String): List<ProseBlock> = buildList {
            val lines = mutableListOf<String>()
            var code: MutableList<String>? = null
            val table = mutableListOf<String>()
            fun flush() {
                if (table.isNotEmpty()) {
                    add(Table(table.joinToString("\n")))
                    table.clear()
                }
                val joined = lines.joinToString("\n").trim('\n', '\r')
                if (joined.isNotEmpty()) add(Lines(joined))
                lines.clear()
            }
            for (line in text.split('\n')) {
                val trimmed = line.trimStart(' ')
                val open = code
                when {
                    trimmed.startsWith("```") || trimmed.startsWith("~~~") -> if (open != null) {
                        add(Code(open.joinToString("\n")))
                        code = null
                    } else {
                        flush()
                        code = mutableListOf()
                    }

                    open != null -> open += line

                    trimmed.startsWith("|") -> {
                        if (table.isEmpty()) flush()
                        table += line
                    }

                    else -> {
                        val heading = heading(trimmed)
                        if (heading != null) {
                            flush()
                            add(Heading(heading))
                        } else {
                            if (table.isNotEmpty()) flush()
                            lines += bulleted(line)
                        }
                    }
                }
            }
            code?.let { add(Code(it.joinToString("\n"))) }
            flush()
        }

        private fun heading(line: String): String? {
            val marks = line.takeWhile { it == '#' }.length
            if (marks !in 1..6 || line.getOrNull(marks) != ' ') return null
            return line.substring(marks + 1)
        }

        private fun bulleted(line: String): String {
            val indent = line.takeWhile { it == ' ' }
            val rest = line.substring(indent.length)
            if (!rest.startsWith("- ") && !rest.startsWith("* ") && !rest.startsWith("+ ")) return line
            return indent + "• " + rest.substring(2)
        }
    }
}

/** An agent's answer. Agents write Markdown: a block of code sinks into a well, a heading stands out. */
@Composable
internal fun ChatProse(text: String, live: Boolean, modifier: Modifier = Modifier) {
    val colors = KodosiTheme.colors
    val blocks = remember(text) { ProseBlock.blocks(text) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(9.dp)) {
        blocks.forEachIndexed { index, block ->
            when (block) {
                is ProseBlock.Lines -> ProseLines(block.text, live && index == blocks.lastIndex)

                is ProseBlock.Heading -> SelectionContainer {
                    Text(
                        rememberWords(block.text, true),
                        style = KodosiType.subhead,
                        color = colors.ink,
                        modifier = Modifier.padding(top = if (index == 0) 0.dp else 3.dp)
                    )
                }

                is ProseBlock.Code -> CodeWell(Modifier.fillMaxWidth()) {
                    SelectionContainer {
                        Text(
                            block.text,
                            style = ChatType.mono,
                            color = colors.ink,
                            softWrap = false,
                            modifier = Modifier.horizontalScroll(rememberScrollState())
                        )
                    }
                }

                is ProseBlock.Table -> SelectionContainer {
                    Text(
                        block.text,
                        style = ChatType.mono,
                        color = colors.inkMuted,
                        softWrap = false,
                        modifier = Modifier.horizontalScroll(rememberScrollState())
                    )
                }
            }
        }
        if (live && blocks.lastOrNull() !is ProseBlock.Lines) TypingCursor(Modifier.size(6.dp, 14.dp))
    }
}

@Composable
private fun ProseLines(text: String, live: Boolean) {
    val colors = KodosiTheme.colors
    val words = rememberWords(text, true)
    val shown = remember(words, live) {
        if (live) {
            buildAnnotatedString {
                append(words)
                append(' ')
                appendInlineContent(CURSOR, "▍")
            }
        } else {
            words
        }
    }
    SelectionContainer {
        Text(
            shown,
            style = KodosiType.body,
            color = colors.ink,
            inlineContent = if (live) cursor else emptyMap()
        )
    }
}

private const val CURSOR = "cursor"

private val cursor = mapOf(
    CURSOR to InlineTextContent(Placeholder(6.sp, 14.sp, PlaceholderVerticalAlign.TextCenter)) {
        TypingCursor(Modifier.fillMaxSize())
    }
)
