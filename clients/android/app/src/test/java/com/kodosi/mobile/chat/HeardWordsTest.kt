package com.kodosi.mobile.chat

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class HeardWordsTest {
    @Test
    fun wordsThatLeftWhileTentativeStayOutOfTheNextMessage() {
        val sent = HeardWords().settling("Fix the").hearing("login").hearing("login test").sent()

        val finished = sent.hearing("login test and then").settling("login test.").hearing("Then run it")
        assertEquals("Then run it", finished.message)

        val stopped = sent.stopping().settling("Then run it.")
        assertEquals("Then run it.", stopped.message)
    }

    @Test
    fun settlingWordsWithoutChangingThemKeepsThePauseAndTheNextMessageStartsAtItsFirstWord() {
        val heard = HeardWords().hearing("fix the login test")
        val settled = heard.settling("Fix the login test.")

        assertNotEquals(HeardWords().spoken, heard.spoken)
        assertEquals(heard.spoken, settled.spoken)
        assertEquals("Fix the login test.", settled.message)
        assertEquals("Then run the tests again.", settled.sent().settling(". Then run the tests again.").message)
        assertEquals("¿Y los tests?", settled.sent().settling(" ¿Y los tests?").message)
    }

    @Test
    fun speechThatTheRecognizerSettlesAsPunctuationAloneKeepsTheWordsThatItHeard() {
        val words = HeardWords().hearing("Then run the tests again").settling(".....")

        assertEquals("Then run the tests again", words.message)
    }

    @Test
    fun spokenPhrasesJoinWithSpacesExceptBeforePunctuationAndInChineseAndJapanese() {
        assertEquals("Fix the bug and run the tests", joinedWords("Fix the bug", "and run the tests"))
        assertEquals("Fix the bug.", joinedWords("Fix the bug", "."))
        assertEquals("バグを直して。テストを実行", joinedWords("バグを直して。", "テストを実行"))
        assertEquals("修复这个错误", joinedWords("修复", "这个错误"))
        assertEquals("버그를 고쳐 주세요", joinedWords("버그를", "고쳐 주세요"))
        assertEquals("Hola. ¿Qué tal?", joinedWords("Hola.", "¿Qué tal?"))
    }
}
