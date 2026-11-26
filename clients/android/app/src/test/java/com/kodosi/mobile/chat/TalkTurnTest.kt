package com.kodosi.mobile.chat

import com.kodosi.mobile.chat.TalkTurn.Step
import kotlin.math.roundToLong
import org.junit.Assert.assertEquals
import org.junit.Test

class TalkTurnTest {
    @Test
    fun aLongQuietAfterThePersonStopsTalkingSendsTheirWordsAndAThinkingPauseDoesNot() {
        val turn = TalkTurn(at(0.0))
        turn.sound(-60.0, at(0.0))
        turn.hearing("Fix the login test", at(1.2), holding = false)
        turn.sound(-20.0, at(1.3))
        assertEquals(Step.Wait, turn.tick(at(3.3), holding = false))

        turn.sound(-20.0, at(3.4))
        turn.hearing("Fix the login test and keep the old error", at(4.5), holding = false)
        turn.settling("Fix the login test and keep the old error.", at(5.0), holding = false)
        assertEquals(Step.Wait, turn.tick(at(7.4), holding = false))
        assertEquals(Step.Send("Fix the login test and keep the old error."), turn.tick(at(7.6), holding = false))
    }

    @Test
    fun whileTheAgentAsksAQuestionOnlySendSendsTheWordsAndTheyWaitAWholePauseAfterItCloses() {
        val turn = TalkTurn(at(0.0))
        turn.settling("Use the staging database", at(2.0), holding = false)
        assertEquals(Step.Wait, turn.tick(at(10.0), holding = true))
        assertEquals(Step.Wait, turn.tick(at(11.0), holding = false))
        assertEquals(Step.Wait, turn.tick(at(13.9), holding = false))
        assertEquals(Step.Send("Use the staging database"), turn.tick(at(14.1), holding = false))

        turn.settling("No, use the local one", at(16.0), holding = false)
        turn.tick(at(30.0), holding = true)
        assertEquals(Step.Send("No, use the local one"), turn.askToSend(at(30.1), holding = true))
    }

    @Test
    fun aSendWaitsForTheRecognizerToSettleTheLastWordsButNotForLong() {
        val turn = TalkTurn(at(0.0))
        turn.hearing("Run the tests", at(1.0), holding = false)
        assertEquals(Step.Wait, turn.askToSend(at(1.1), holding = false))
        assertEquals(Step.Send("Run the tests again."), turn.settling("Run the tests again.", at(1.8), holding = false))

        turn.hearing("and deploy", at(3.0), holding = false)
        assertEquals(Step.Wait, turn.askToSend(at(3.1), holding = false))
        assertEquals(Step.Wait, turn.tick(at(5.0), holding = false))
        assertEquals(Step.Send("and deploy"), turn.tick(at(5.2), holding = false))
    }

    @Test
    fun talkingEndsAfterTwoMinutesWithoutWords() {
        val turn = TalkTurn(at(0.0))
        assertEquals(Step.Wait, turn.tick(at(119.0), holding = false))
        assertEquals(Step.End, turn.tick(at(121.0), holding = false))
    }

    @Test
    fun aQuestionThatOpensWhileTheLastWordsSettleStopsTheirSendAndSendThenAnswersIt() {
        val turn = TalkTurn(at(0.0))
        turn.hearing("Use the staging database", at(1.0), holding = false)
        assertEquals(Step.Wait, turn.tick(at(4.0), holding = false))
        assertEquals(Step.Wait, turn.tick(at(4.5), holding = true))
        assertEquals(Step.Wait, turn.settling("Use the staging database.", at(4.8), holding = true))
        assertEquals(Step.Wait, turn.tick(at(7.0), holding = true))
        assertEquals(Step.Send("Use the staging database."), turn.askToSend(at(7.1), holding = true))
    }

    private fun at(seconds: Double): Long = START + (seconds * 1000).roundToLong()

    private companion object {
        const val START = 1_800_000_000L
    }
}
