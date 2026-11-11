package com.kodosi.mobile.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LinkPolicyTest {
    @Test
    fun keepsWebAndMailLinksButReviewsOtherApps() {
        assertEquals(LinkReview.Open("https://bücher.example/a", false), LinkPolicy.review("https://bücher.example/a"))
        assertEquals(LinkReview.Open("mailto:ana@example.org", false), LinkPolicy.review("mailto:ana@example.org"))
        assertEquals(LinkReview.Open("vscode://file/code", true), LinkPolicy.review("vscode://file/code"))
        listOf(
            "javascript:alert(1)",
            "data:text/html,test",
            "intent://settings",
            "content://private/file",
            "https://ana:secret@example.org",
            "https://example.org/\u202eabc",
            "https://example.org/\n"
        ).forEach { assertEquals(it, LinkReview.Invalid, LinkPolicy.review(it)) }
    }

    @Test
    fun keepsComputerLocalLinksOnTheirComputer() {
        listOf("http://localhost:3000", "http://127.0.0.1:8000", "http://[::1]:3000", "file:///code/report.html")
            .forEach { assertEquals(it, LinkReview.OnComputer("Mac"), LinkPolicy.review(it, "Mac")) }
        assertTrue(LinkPolicy.review("https://example.org", "Mac") is LinkReview.Open)
    }

    @Test
    fun detectsCompleteLinksWithoutSentencePunctuation() {
        val text = "Read (https://example.org/a(b)), then mailto:ana@example.org."
        val links = LinkPolicy.inText(text).toList()
        assertEquals(listOf("https://example.org/a(b)", "mailto:ana@example.org"), links.map { it.address })
        assertEquals(links.map { it.address }, links.map { text.substring(it.range) })
    }
}
