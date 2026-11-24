package com.kodosi.mobile.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class LinkPolicyTest {
    @Test
    fun detectsCompleteLinksWithoutSentencePunctuation() {
        val text = "Read (https://example.org/a(b)), then mailto:ana@example.org."
        val links = LinkPolicy.inText(text).toList()
        assertEquals(listOf("https://example.org/a(b)", "mailto:ana@example.org"), links.map { it.address })
        assertEquals(links.map { it.address }, links.map { text.substring(it.range) })
    }
}
