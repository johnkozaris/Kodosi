package com.kodosi.mobile.ui

// Minimal inline Markdown parser that preserves literal spaces and line breaks.
object InlineMarkdown {
    data class Run(
        val text: String,
        val strong: Boolean = false,
        val emphasis: Boolean = false,
        val code: Boolean = false,
        val struck: Boolean = false,
        val link: String? = null
    )

    private class NamedLink(val label: String, val address: String, val end: Int)

    private const val ESCAPED = "\\`*_{}[]()#+-.!~>|"
    private val address = Regex("""(?:https?://|mailto:)[^\s<>"'\\()]+""", RegexOption.IGNORE_CASE)

    fun runs(text: String): List<Run> = buildList { read(text, Run(""), this) }

    private fun read(text: String, style: Run, out: MutableList<Run>) {
        val plain = StringBuilder()
        fun flush() {
            if (plain.isNotEmpty()) {
                out += style.copy(text = plain.toString())
                plain.clear()
            }
        }
        var at = 0
        while (at < text.length) {
            val char = text[at]
            when {
                char == '\\' && at + 1 < text.length && text[at + 1] in ESCAPED -> {
                    plain.append(text[at + 1])
                    at += 2
                }

                char == '`' -> {
                    val ticks = run(text, at)
                    val close = codeEnd(text, at + ticks, ticks)
                    if (close < 0) {
                        plain.append(text, at, at + ticks)
                        at += ticks
                    } else {
                        flush()
                        out += style.copy(text = code(text.substring(at + ticks, close)), code = true)
                        at = close + ticks
                    }
                }

                char == '[' && style.link == null -> {
                    val link = namedLink(text, at)
                    if (link == null) {
                        plain.append(char)
                        at += 1
                    } else {
                        flush()
                        read(link.label, style.copy(link = link.address), out)
                        at = link.end
                    }
                }

                char == '*' || char == '_' || char == '~' -> {
                    val size = minOf(run(text, at), 2)
                    val close = if ((char == '~' && size < 2) || !opens(text, at, size)) {
                        -1
                    } else {
                        closing(text, at + size, char, size)
                    }
                    if (close < 0) {
                        plain.append(text, at, at + size)
                        at += size
                    } else {
                        flush()
                        val inner = when {
                            char == '~' -> style.copy(struck = true)
                            size == 2 -> style.copy(strong = true)
                            else -> style.copy(emphasis = true)
                        }
                        read(text.substring(at + size, close), inner, out)
                        at = close + size
                    }
                }

                else -> {
                    plain.append(char)
                    at += 1
                }
            }
        }
        flush()
    }

    private fun run(text: String, at: Int): Int {
        var end = at
        while (end < text.length && text[end] == text[at]) end += 1
        return end - at
    }

    private fun codeEnd(text: String, from: Int, ticks: Int): Int {
        var at = from
        while (at < text.length) {
            if (text[at] != '`') {
                at += 1
                continue
            }
            val length = run(text, at)
            if (length == ticks) return at
            at += length
        }
        return -1
    }

    private fun code(inside: String): String =
        if (inside.length > 2 && inside.startsWith(' ') && inside.endsWith(' ') && inside.isNotBlank()) {
            inside.substring(1, inside.length - 1)
        } else {
            inside
        }

    private fun opens(text: String, at: Int, size: Int): Boolean {
        val next = text.getOrNull(at + size) ?: return false
        return !next.isWhitespace() && (text[at] != '_' || at == 0 || !text[at - 1].isLetterOrDigit())
    }

    // Require an odd run length for single-mark closing so nested **strong** spans stay intact.
    private fun closing(text: String, from: Int, mark: Char, size: Int): Int {
        var at = from
        while (at < text.length) {
            val char = text[at]
            when {
                char == '\\' -> at += 2

                char == '`' -> {
                    val ticks = run(text, at)
                    val end = codeEnd(text, at + ticks, ticks)
                    at = if (end < 0) at + ticks else end + ticks
                }

                char == mark -> {
                    val length = run(text, at)
                    val after = at + length
                    val fits = if (size == 1) length % 2 == 1 else length >= 2
                    val closes = fits && at > from && !text[at - 1].isWhitespace() &&
                        (mark != '_' || after >= text.length || !text[after].isLetterOrDigit())
                    if (closes) return after - size
                    at = after
                }

                else -> at += 1
            }
        }
        return -1
    }

    private fun namedLink(text: String, at: Int): NamedLink? {
        val labelEnd = text.indexOf(']', at + 1)
        if (labelEnd <= at + 1 || text.getOrNull(labelEnd + 1) != '(') return null
        val end = text.indexOf(')', labelEnd + 2)
        if (end < 0) return null
        val label = text.substring(at + 1, labelEnd)
        val target = text.substring(labelEnd + 2, end)
        if ('\n' in label || !address.matches(target)) return null
        return NamedLink(label, target, end + 1)
    }
}
