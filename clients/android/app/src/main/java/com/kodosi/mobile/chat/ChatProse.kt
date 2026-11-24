package com.kodosi.mobile.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LocalLinkOpener
import com.kodosi.mobile.ui.rememberWords
import java.net.URI

internal sealed interface ProseBlock {
    data class Lines(val text: String) : ProseBlock

    data class Heading(val text: String) : ProseBlock

    data class Code(val text: String) : ProseBlock

    data class Table(val table: ProseTable) : ProseBlock

    companion object {
        /** A fence that is still open while the agent writes holds the rest. */
        fun blocks(text: String): List<ProseBlock> = buildList {
            val lines = mutableListOf<String>()
            var code: MutableList<String>? = null
            val table = mutableListOf<String>()
            fun flush() {
                if (table.isNotEmpty()) {
                    add(Table(ProseTable.of(table)))
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

/**
 * A table as an agent writes it in Markdown. The first row names the columns when a row of dashes
 * follows it, and that row says on which side each column stands.
 */
internal data class ProseTable(val header: List<String>, val rows: List<List<String>>, val sides: List<Side>) {
    enum class Side { START, CENTER, END }

    val columns: Int get() = maxOf(header.size, rows.maxOfOrNull { it.size } ?: 0)

    fun side(column: Int): Side = sides.getOrElse(column) { Side.START }

    companion object {
        fun of(lines: List<String>): ProseTable {
            val rows = lines.map(::cells)
            val sides = rows.getOrNull(1)?.let(::sides)
            return if (sides != null) {
                ProseTable(rows[0], rows.drop(2), sides)
            } else {
                ProseTable(emptyList(), rows, emptyList())
            }
        }

        /**
         * A bar after a backslash belongs to the words of its cell, and the bars at the two ends of
         * a row hold no cell.
         */
        fun cells(line: String): List<String> {
            val cells = mutableListOf<String>()
            val cell = StringBuilder()
            var escaped = false
            for (character in line) {
                when {
                    escaped -> {
                        if (character != '|') cell.append('\\')
                        cell.append(character)
                        escaped = false
                    }

                    character == '\\' -> escaped = true

                    character == '|' -> {
                        cells += cell.toString()
                        cell.clear()
                    }

                    else -> cell.append(character)
                }
            }
            if (escaped) cell.append('\\')
            cells += cell.toString()
            val words = cells.map { it.trim(' ', '\t') }.toMutableList()
            if (words.firstOrNull()?.isEmpty() == true) words.removeAt(0)
            if (words.lastOrNull()?.isEmpty() == true) words.removeAt(words.lastIndex)
            return words
        }

        private fun sides(row: List<String>): List<Side>? {
            if (row.isEmpty() || !row.all { cell -> '-' in cell && cell.all { it in "-: " } }) return null
            return row.map { cell ->
                when {
                    cell.startsWith(':') && cell.endsWith(':') -> Side.CENTER
                    cell.endsWith(':') -> Side.END
                    else -> Side.START
                }
            }
        }
    }
}

/** An agent's answer. Agents write Markdown: a block of code sinks into a well, a heading stands out, a table has its columns. */
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

                is ProseBlock.Table -> ProseTableView(block.table)
            }
        }
        if (live && blocks.lastOrNull() !is ProseBlock.Lines) TypingCursor(Modifier.size(6.dp, 14.dp))
    }
}

/** A table wider than the conversation moves sideways, and a long cell breaks its lines. */
@Composable
private fun ProseTableView(table: ProseTable) {
    val colors = KodosiTheme.colors
    val named = table.header.isNotEmpty()
    val rows = remember(table) { if (named) listOf(table.header) + table.rows else table.rows }
    val columns = table.columns
    SelectionContainer {
        Layout(
            content = {
                rows.forEachIndexed { row, words ->
                    repeat(columns) { column ->
                        key(row, column) {
                            Text(
                                rememberWords(words.getOrElse(column) { "" }, true),
                                style = KodosiType.body,
                                fontWeight = if (named && row == 0) FontWeight.SemiBold else null,
                                color = colors.ink,
                                textAlign = when (table.side(column)) {
                                    ProseTable.Side.START -> TextAlign.Start
                                    ProseTable.Side.CENTER -> TextAlign.Center
                                    ProseTable.Side.END -> TextAlign.End
                                }
                            )
                        }
                    }
                }
                if (named) Box(Modifier.background(colors.hairline))
            },
            modifier = Modifier.horizontalScroll(rememberScrollState())
        ) { measurables, _ ->
            val gap = TableGap.roundToPx()
            val space = TableSpace.roundToPx()
            val line = TableLine.roundToPx().coerceAtLeast(1)
            val cell = Constraints(maxWidth = WidestCell.roundToPx())
            val cells = measurables.take(rows.size * columns).map { it.measure(cell) }.chunked(columns.coerceAtLeast(1))
            val widths = List(columns) { column -> cells.maxOfOrNull { it[column].width } ?: 0 }
            val heights = cells.map { row -> row.maxOfOrNull { it.height } ?: 0 }
            val width = widths.sum() + gap * (columns - 1).coerceAtLeast(0)
            val under = if (named) measurables.last().measure(Constraints.fixed(width, line)) else null
            val height = heights.sum() + space * (heights.size - 1).coerceAtLeast(0) +
                if (under != null) line + space else 0
            layout(width, height) {
                var y = 0
                cells.forEachIndexed { row, placed ->
                    var x = 0
                    placed.forEachIndexed { column, words ->
                        val free = widths[column] - words.width
                        val start = when (table.side(column)) {
                            ProseTable.Side.START -> 0
                            ProseTable.Side.CENTER -> free / 2
                            ProseTable.Side.END -> free
                        }
                        words.placeRelative(x + start, y)
                        x += widths[column] + gap
                    }
                    y += heights[row] + space
                    if (row == 0 && under != null) {
                        under.placeRelative(0, y)
                        y += line + space
                    }
                }
            }
        }
    }
}

private val TableGap = 18.dp
private val TableSpace = 7.dp
private val TableLine = 0.5.dp
private val WidestCell = 260.dp

@Composable
private fun ProseLines(text: String, live: Boolean) {
    val colors = KodosiTheme.colors
    val folder = LocalProseFolder.current
    val links = LocalLinkOpener.current
    val marked = rememberWords(text, true)
    val words = remember(marked, folder, links, colors.accent) {
        ProsePaths.linked(
            marked,
            folder,
            TextLinkStyles(SpanStyle(color = colors.accent, textDecoration = TextDecoration.Underline))
        ) {
            links.open(it)
        }
    }
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

/** The folder of the terminal, from which a path in an agent's prose names a file. */
internal val LocalProseFolder = staticCompositionLocalOf<String?> { null }

/**
 * A path in an agent's prose is a link to its file, as Ghostty finds one: names with slashes
 * between them, and a line after a colon. A path that does not start at a folder ends with the
 * extension of a file, so words such as "and/or" and numbers such as 3/4 stay text.
 */
internal object ProsePaths {
    private val path = Regex("""(?<![\w/.:@-])(?:~/|\.{1,2}/|/)?(?:[\w.-]+/)+[\w.-]+(?::\d+){0,2}""")
    private val line = Regex("""(?::\d+){1,2}$""")
    private val extension = Regex("""\.[A-Za-z0-9]*[A-Za-z][A-Za-z0-9]*$""")

    fun linked(
        words: AnnotatedString,
        folder: String?,
        style: TextLinkStyles,
        open: (String) -> Unit
    ): AnnotatedString {
        val found = path.findAll(words.text).mapNotNull { match ->
            val end = match.range.last + 1
            file(match.value, folder)?.takeIf { words.getLinkAnnotations(match.range.first, end).isEmpty() }
                ?.let { match.range.first until end to it }
        }.toList()
        if (found.isEmpty()) return words
        return buildAnnotatedString {
            append(words)
            for ((range, address) in found) {
                addLink(LinkAnnotation.Clickable(address, style) { open(address) }, range.first, range.last + 1)
            }
        }
    }

    fun file(found: String, folder: String?): String? {
        val path = found.replace(line, "")
        val rooted = listOf("/", "~/", "./", "../").any { path.startsWith(it) }
        if (!rooted && !extension.containsMatchIn(path.substringAfterLast('/'))) return null
        return when {
            path.startsWith("/") -> URI("file", null, path, null).normalize().toString()
            path.startsWith("~/") -> "file:$path"
            folder == null -> null
            else -> URI("file", null, "${folder.trimEnd('/')}/$path", null).normalize().toString()
        }
    }
}
