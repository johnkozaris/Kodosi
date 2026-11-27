package com.kodosi.mobile.chat

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChatMentionMatchTest {
    private val peers =
        listOf("Timber Elk", "Slate Wren", "Amber Owl", "Iron Lark", "Jade Fox", "Moss Elk", "Elk Ridge")
            .map { ChatPeer(it.lowercase().replace(' ', '-'), it, "claude") }

    @Test
    fun onlyAnAtThatStartsAWordOnOneLineOfAtMostThreeWordsAsksForAPeer() {
        assertEquals("", ChatMentionMatch.query("Ask @"))
        assertEquals("Timber E", ChatMentionMatch.query("(@Timber E"))
        assertEquals("tim", ChatMentionMatch.query("ask @slate wren and @tim"))
        assertEquals("a b c", ChatMentionMatch.query("@a b c"))
        assertNull(ChatMentionMatch.query("mail ann@tim"))
        assertNull(ChatMentionMatch.query("snake_@tim"))
        assertNull(ChatMentionMatch.query("@Timber\nElk"))
        assertNull(ChatMentionMatch.query("@a b c d"))
        assertNull(ChatMentionMatch.query("@" + "e".repeat(41)))
        assertNull(ChatMentionMatch.query("no at"))
    }

    @Test
    fun namesThatStartWithTheWordsComeBeforeNamesThatHoldThemAndSixAtMost() {
        val named = ChatMentionMatch.peers(peers, "ELK").map { it.name }
        assertEquals(listOf("Elk Ridge", "Timber Elk", "Moss Elk"), named)
        assertEquals(peers.take(6), ChatMentionMatch.peers(peers, ""))
        assertEquals(emptyList<ChatPeer>(), ChatMentionMatch.peers(peers, "Timber Elk "))
    }

    @Test
    fun aPeerTakesThePlaceOfTheWordsAfterTheLastAtAndAHandOffTakesThemAway() {
        val draft = "Ask @Slate Wren and @tim"
        assertEquals("Ask @Slate Wren and @Timber Elk ", ChatMentionMatch.naming(draft, peers[0]))
        assertEquals("Ask @Slate Wren and ", ChatMentionMatch.unnamed(draft))
    }

    @Test
    fun theWordsNameAnAgentWithAnAtThatStartsAWordAndTheLongestNameThatEndsOne() {
        val text = "Ask @timber elk 2, then @Timber Elk; not ann@Timber Elk or @Timber Elks."
        val mentions = listOf(ChatMention("Timber Elk", "copilot"), ChatMention("Timber Elk 2", "codex"))

        assertEquals(
            listOf("@timber elk 2" to "Timber Elk 2", "@Timber Elk" to "Timber Elk"),
            ChatMentionMatch.named(text, mentions).map { (range, mention) -> text.substring(range) to mention.name }
        )
    }
}
