package com.kodosi.mobile.chat

/**
 * The words of the next message while talk mode listens: the [settled] words that the recognizer
 * finished, then the [tentative] words of the segment that it still hears and may change. When a
 * message leaves with tentative words, the rest of their segment stays out of the next message.
 */
internal data class HeardWords(
    val settled: String = "",
    val tentative: String = "",
    private val segmentSent: Boolean = false
) {
    val message: String get() = joinedWords(settled, tentative)

    val isEmpty: Boolean get() = message.isBlank()

    /**
     * The letters and digits of the message, which change only when the spoken words change and
     * not when the recognizer adds punctuation or capitals as it settles them.
     */
    val spoken: String get() = spokenOf(message)

    /** The recognizer's newest guess at the segment that it hears now. */
    fun hearing(words: String): HeardWords = if (segmentSent) this else copy(tentative = opening(words))

    /** The recognizer finished the segment that it heard. */
    fun settling(words: String): HeardWords {
        if (segmentSent) return copy(tentative = "", segmentSent = false)
        // A recognizer can settle speech as punctuation alone; the words that it heard stay.
        val heard = opening(words).takeIf { spokenOf(it).isNotEmpty() } ?: tentative
        return copy(settled = joinedWords(settled, heard), tentative = "")
    }

    /** The recognizer stopped listening, so no later words finish the segment that it heard. */
    fun stopping(): HeardWords = if (segmentSent) copy(segmentSent = false) else settling(tentative)

    /** What is left once the message left. */
    fun sent(): HeardWords = HeardWords(segmentSent = segmentSent || tentative.isNotEmpty())

    /** A message starts without the space or the full stop that ends the one before it. */
    private fun opening(words: String): String {
        val trimmed = words.trim()
        if (settled.isNotEmpty()) return trimmed
        return trimmed.trimStart { it.isWhitespace() || it.code.isClosingPunctuation() }
    }
}

private fun spokenOf(words: String): String = buildString {
    words.lowercase().codePoints().forEach { if (Character.isLetterOrDigit(it)) appendCodePoint(it) }
}

/**
 * Two runs of words joined with a space, except before punctuation, where a space already is, or in
 * the scripts that write words without spaces.
 */
internal fun joinedWords(before: String, after: String): String {
    if (before.isEmpty()) return after
    if (after.isEmpty()) return before
    val last = before.codePointBefore(before.length)
    val first = after.codePointAt(0)
    val close = Character.isWhitespace(last) || Character.isWhitespace(first) || first.isClosingPunctuation() ||
        last.writesWithoutSpaces() || first.writesWithoutSpaces()
    return if (close) before + after else "$before $after"
}

/** Chinese characters and Japanese kana, with their punctuation. */
private fun Int.writesWithoutSpaces(): Boolean = this in 0x3000..0x30FF || this in 0x31F0..0x31FF ||
    this in 0x3400..0x4DBF || this in 0x4E00..0x9FFF || this in 0xF900..0xFAFF || this in 0xFF00..0xFFEF ||
    this in 0x20000..0x2FA1F

// Marks that open a phrase, such as ( “ and the Spanish ¿ ¡, belong to the words after them.
private fun Int.isClosingPunctuation(): Boolean = when (Character.getType(this).toByte()) {
    Character.CONNECTOR_PUNCTUATION, Character.DASH_PUNCTUATION, Character.END_PUNCTUATION,
    Character.FINAL_QUOTE_PUNCTUATION -> true

    Character.OTHER_PUNCTUATION -> this != '¿'.code && this != '¡'.code

    else -> false
}
