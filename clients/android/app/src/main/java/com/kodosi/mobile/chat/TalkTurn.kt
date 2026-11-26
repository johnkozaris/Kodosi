package com.kodosi.mobile.chat

/**
 * When a spoken message goes out. A long quiet after the person stops talking sends their words,
 * because people stop to think while they talk; Send sends them sooner. Either way they go once
 * the recognizer settled the last of them, or after a short wait for it. Times are uptime
 * milliseconds.
 */
internal class TalkTurn(now: Long) {
    sealed interface Step {
        data object Wait : Step

        data class Send(val message: String) : Step

        /** Nobody talked for a long time. */
        data object End : Step
    }

    var words = HeardWords()
        private set

    /**
     * When the person last talked or their words last changed, while words wait to go out; the
     * pause that sends them counts from here.
     */
    var quietSince: Long? = null
        private set

    /** When a send was asked for; the words go once the recognizer settled them. */
    var sendAsked: Long? = null
        private set

    /** The send was asked for while the agent asked a question, so the words answer it. */
    private var sendAnswers = false
    private val voice = TalkVoice()
    private var lastWord = now
    private var holding = false

    /**
     * The recognizer's newest guess at the segment that it hears now. Every step takes [holding]:
     * the agent asks a question, which words would answer.
     */
    fun hearing(heard: String, now: Long, holding: Boolean): Step = heard(words.hearing(heard), now, holding)

    /** The recognizer finished the segment that it heard. */
    fun settling(heard: String, now: Long, holding: Boolean): Step = heard(words.settling(heard), now, holding)

    /** The recognizer stopped listening, so no later words finish the segment that it heard. */
    fun stopping(now: Long, holding: Boolean): Step = heard(words.stopping(), now, holding)

    /** Takes the loudness of the latest sound, in decibels of the microphone's full scale. */
    fun sound(decibels: Double, now: Long) {
        if (voice.hears(decibels, now) && !words.isEmpty) quietSince = now
    }

    /** The person asks to send the words now. */
    fun askToSend(now: Long, holding: Boolean): Step {
        hold(holding, now)
        if (words.isEmpty || sendAsked != null) return Step.Wait
        sendAsked = now
        sendAnswers = holding
        return deliverIfSettled(now)
    }

    /** Moves time on: a long quiet asks to send the words, unless the agent asks a question. */
    fun tick(now: Long, holding: Boolean): Step {
        hold(holding, now)
        val quiet = quietSince
        if (quiet != null && !holding && sendAsked == null && !words.isEmpty && now - quiet >= PAUSE_MILLIS) {
            sendAsked = now
            sendAnswers = false
        }
        val step = deliverIfSettled(now)
        return if (step == Step.Wait && now - lastWord >= IDLE_MILLIS) Step.End else step
    }

    /** The words that did not go out, which end this turn. */
    fun takeUnsent(): String {
        quietSince = null
        sendAsked = null
        return take()
    }

    /**
     * A question that opens stops a send that was not asked for as its answer, since words sent now
     * would answer it; once it closes, the words wait a whole pause again.
     */
    private fun hold(holding: Boolean, now: Long) {
        if (holding && sendAsked != null && !sendAnswers) sendAsked = null
        if (this.holding && !holding && quietSince != null) quietSince = now
        this.holding = holding
    }

    private fun heard(next: HeardWords, now: Long, holding: Boolean): Step {
        hold(holding, now)
        if (next.spoken != words.spoken) {
            quietSince = now
            lastWord = now
        }
        words = next
        return deliverIfSettled(now)
    }

    private fun deliverIfSettled(now: Long): Step {
        val asked = sendAsked ?: return Step.Wait
        if (words.tentative.isNotEmpty() && now - asked < SETTLE_MILLIS) return Step.Wait
        sendAsked = null
        if (words.isEmpty) return Step.Wait
        quietSince = null
        lastWord = now
        return Step.Send(take())
    }

    private fun take(): String {
        val message = words.message.trim()
        words = words.sent()
        return message
    }

    companion object {
        /** The quiet after the person stops talking that sends their words. */
        const val PAUSE_MILLIS = 3_000L

        /** How long talking lasts without words. */
        const val IDLE_MILLIS = 120_000L

        private const val SETTLE_MILLIS = 2_000L
    }
}
