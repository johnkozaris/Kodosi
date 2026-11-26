package com.kodosi.mobile.chat

import android.Manifest
import android.app.LocaleManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.SystemClock
import android.speech.ModelDownloadListener
import android.speech.RecognitionListener
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Log
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.agentLabel
import java.util.Locale
import kotlinx.coroutines.delay

private const val TICK_MILLIS = 100L
private const val MODEL_CHECK_MILLIS = 2_000L

private const val TAG = "Kodosi"
private const val UNSTABLE_TEXT = "android.speech.extra.UNSTABLE_TEXT"

/** Why talk mode can't listen, said above the composer until the next try. */
internal enum class TalkNote { MICROPHONE, UNAVAILABLE, FAILED }

/**
 * Talk mode of a terminal's chat: the device recognizes the person's speech itself, with a
 * recognizer that lives only while talk mode is on, and [TalkTurn] decides when the words go out.
 * [asking] tells whether the agent asks a question now, which words would answer.
 */
@Stable
internal class ChatTalk(
    private val context: Context,
    private val submit: (String) -> Boolean,
    private val keep: (String) -> Unit,
    private val asking: () -> Boolean
) {
    var on by mutableStateOf(false)
        private set

    /** Whether the recognizer started to listen in this talk mode; until then talk mode gets ready. */
    var listening by mutableStateOf(false)
        private set

    var words by mutableStateOf(HeardWords())
        private set

    /** When the person last talked or their words last changed, while a pause can send them. */
    var quietSince by mutableStateOf<Long?>(null)
        private set

    /** When a send was asked for, while the words wait for the recognizer to settle them. */
    var sendAsked by mutableStateOf<Long?>(null)
        private set

    var note by mutableStateOf<TalkNote?>(null)

    /** How loud the recognizer hears the person, from 0 to 1. */
    var level by mutableFloatStateOf(0f)
        private set

    var hints: List<String> = emptyList()

    val recognizes: Boolean get() = SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

    val permitted: Boolean
        get() = context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private var turn = TalkTurn(0L)
    private var recognizer: SpeechRecognizer? = null
    private var language = ""
    private var ready = false

    /** When talk mode last asked whether its language is installed, while it waits for the download. */
    private var modelCheckedAt: Long? = null

    fun start() {
        if (on) return
        val starting = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        starting.setRecognitionListener(Hearing(starting))
        recognizer = starting
        on = true
        listening = false
        note = null
        turn = TalkTurn(SystemClock.uptimeMillis())
        publish()
        val wanted = (context.getSystemService(LocaleManager::class.java).systemLocales[0] ?: Locale.getDefault())
            .stripExtensions()
        language = wanted.toLanguageTag()
        starting.checkRecognitionSupport(
            intent(),
            context.mainExecutor,
            object : RecognitionSupportCallback {
                override fun onSupportResult(support: RecognitionSupport) {
                    if (recognizer === starting) prepare(wanted, support)
                }

                override fun onError(error: Int) {
                    if (recognizer === starting) listen()
                }
            }
        )
    }

    /** Ends talk mode; the words that did not go out go to the composer. */
    fun end() {
        if (on) finish(null, turn.takeUnsent())
    }

    fun send() {
        if (listening) apply(turn.askToSend(SystemClock.uptimeMillis(), asking()))
    }

    /** Moves talk mode's time on. */
    fun tick() {
        val now = SystemClock.uptimeMillis()
        val checked = modelCheckedAt
        if (checked != null && now - checked >= MODEL_CHECK_MILLIS) checkModel(now)
        apply(turn.tick(now, asking()))
    }

    private fun apply(step: TalkTurn.Step) {
        publish()
        when (step) {
            TalkTurn.Step.Wait -> Unit

            // A chat that no longer takes messages keeps them in the composer.
            is TalkTurn.Step.Send -> if (!submit(step.message)) finish(null, step.message)

            TalkTurn.Step.End -> if (listening) {
                end()
            } else {
                Log.w(TAG, "Talk mode stopped, as speech recognition did not get ready in time.")
                finish(TalkNote.FAILED, turn.takeUnsent())
            }
        }
    }

    // Compose follows copies of the turn's state, which changes outside of its snapshots.
    private fun publish() {
        words = turn.words
        quietSince = turn.quietSince
        sendAsked = turn.sendAsked
    }

    private fun prepare(wanted: Locale, support: RecognitionSupport) {
        val installed = closestLanguage(wanted, support.installedOnDeviceLanguages)
        val missing = closestLanguage(wanted, support.pendingOnDeviceLanguages + support.supportedOnDeviceLanguages)
        val told = support.installedOnDeviceLanguages.isNotEmpty() ||
            support.pendingOnDeviceLanguages.isNotEmpty() ||
            support.supportedOnDeviceLanguages.isNotEmpty()
        when {
            installed != null -> {
                language = installed
                listen()
            }

            missing != null -> {
                language = missing
                download()
            }

            told -> stop(SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED)

            else -> listen()
        }
    }

    /** Downloads the language, and listens once it is installed, as the download says or a check finds. */
    private fun download() {
        val downloading = recognizer ?: return
        modelCheckedAt = SystemClock.uptimeMillis()
        downloading.triggerModelDownload(
            intent(),
            context.mainExecutor,
            object : ModelDownloadListener {
                override fun onProgress(completedPercent: Int) = Unit

                override fun onScheduled() = Unit

                override fun onSuccess() {
                    if (recognizer === downloading && modelCheckedAt != null) listen()
                }

                override fun onError(error: Int) {
                    if (recognizer !== downloading || modelCheckedAt == null) return
                    // Recognizers that can't report on a download still make it when nobody watches.
                    if (error == SpeechRecognizer.ERROR_CANNOT_LISTEN_TO_DOWNLOAD_EVENTS) {
                        downloading.triggerModelDownload(intent())
                    } else {
                        stop(error)
                    }
                }
            }
        )
    }

    private fun checkModel(now: Long) {
        val checking = recognizer ?: return
        modelCheckedAt = now
        checking.checkRecognitionSupport(
            intent(),
            context.mainExecutor,
            object : RecognitionSupportCallback {
                override fun onSupportResult(support: RecognitionSupport) {
                    val installed = support.installedOnDeviceLanguages.any { it.equals(language, ignoreCase = true) }
                    if (recognizer === checking && modelCheckedAt != null && installed) listen()
                }

                override fun onError(error: Int) = Unit
            }
        )
    }

    private fun listen() {
        modelCheckedAt = null
        ready = false
        recognizer?.startListening(intent())
    }

    private fun intent(): Intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
        .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
        .putExtra(RecognizerIntent.EXTRA_ENABLE_FORMATTING, RecognizerIntent.FORMATTING_OPTIMIZE_QUALITY)
        .putExtra(RecognizerIntent.EXTRA_HIDE_PARTIAL_TRAILING_PUNCTUATION, true)
        .putStringArrayListExtra(RecognizerIntent.EXTRA_BIASING_STRINGS, ArrayList(hints))
        // Only a silence as long as talk mode waits for words ends the session, so the recognizer
        // listens on while the person thinks.
        .putExtra(
            RecognizerIntent.EXTRA_SEGMENTED_SESSION,
            RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS
        )
        .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, TalkTurn.IDLE_MILLIS.toInt())

    private fun failed(error: Int) {
        val silent = error == SpeechRecognizer.ERROR_NO_MATCH || error == SpeechRecognizer.ERROR_SPEECH_TIMEOUT
        if (silent && ready) {
            apply(turn.stopping(SystemClock.uptimeMillis(), asking()))
            listen()
        } else {
            stop(error)
        }
    }

    /** Talk mode stops: with a note when it never listened, else quietly with the words kept. */
    private fun stop(error: Int) {
        Log.w(TAG, "Talk mode stopped on speech recognition error $error.")
        finish(
            when {
                listening -> null

                error == SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> TalkNote.MICROPHONE

                error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED ||
                    error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> TalkNote.UNAVAILABLE

                else -> TalkNote.FAILED
            },
            turn.takeUnsent()
        )
    }

    private fun finish(why: TalkNote?, unsent: String) {
        recognizer?.destroy()
        recognizer = null
        modelCheckedAt = null
        on = false
        listening = false
        level = 0f
        note = why
        publish()
        if (unsent.isNotEmpty()) keep(unsent)
    }

    /** What [hearer] hears, until talk mode lets it go. */
    private inner class Hearing(private val hearer: SpeechRecognizer) : RecognitionListener {
        private val current: Boolean get() = hearer === recognizer

        override fun onReadyForSpeech(params: Bundle?) {
            if (!current) return
            ready = true
            if (listening) return
            listening = true
            turn = TalkTurn(SystemClock.uptimeMillis())
            publish()
        }

        override fun onRmsChanged(rmsdB: Float) {
            if (!current || !listening) return
            level = ((rmsdB + 2f) / 12f).coerceIn(0f, 1f)
            turn.sound(rmsdB.fullScaleDecibels(), SystemClock.uptimeMillis())
            quietSince = turn.quietSince
        }

        override fun onPartialResults(partialResults: Bundle?) {
            if (current) apply(turn.hearing(partialResults.partialWords(), SystemClock.uptimeMillis(), asking()))
        }

        override fun onSegmentResults(segmentResults: Bundle) {
            if (current) apply(turn.settling(segmentResults.finalWords(), SystemClock.uptimeMillis(), asking()))
        }

        override fun onResults(results: Bundle?) {
            if (!current) return
            apply(turn.settling(results.finalWords(), SystemClock.uptimeMillis(), asking()))
            listen()
        }

        override fun onEndOfSegmentedSession() {
            if (!current) return
            apply(turn.stopping(SystemClock.uptimeMillis(), asking()))
            listen()
        }

        override fun onError(error: Int) {
            if (current) failed(error)
        }

        override fun onBeginningOfSpeech() = Unit

        override fun onBufferReceived(buffer: ByteArray?) = Unit

        override fun onEndOfSpeech() {
            if (current) level = 0f
        }

        override fun onEvent(eventType: Int, params: Bundle?) = Unit
    }
}

// Google's recognizer gives the newest words of a partial result, which it is least sure of, apart.
private fun Bundle?.partialWords(): String = joinedWords(
    this?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty().trim(),
    this?.getStringArrayList(UNSTABLE_TEXT)?.firstOrNull().orEmpty().trim()
)

private fun Bundle?.finalWords(): String =
    this?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()

// Recognizers report the loudness of speech from about -2 to 10 decibels, where a microphone hears
// about -60 decibels of its full scale in a quiet room and -20 in loud speech.
private fun Float.fullScaleDecibels(): Double = -60.0 + (this + 2.0) * 40.0 / 12.0

/**
 * The language of [offered] closest to [wanted]: the same one, else the same language as spoken in
 * the same country, else as spoken anywhere.
 */
private fun closestLanguage(wanted: Locale, offered: List<String>): String? {
    val spoken = offered.filter { Locale.forLanguageTag(it).spokenLanguage == wanted.spokenLanguage }
    return spoken.firstOrNull { it.equals(wanted.toLanguageTag(), ignoreCase = true) }
        ?: spoken.firstOrNull { Locale.forLanguageTag(it).country == wanted.country }
        ?: spoken.firstOrNull()
}

// Speech recognizers name Mandarin by its own code rather than as Chinese.
private val Locale.spokenLanguage: String get() = if (language == "cmn") "zh" else language

/** Names that the person may say: the agent's, the terminal's, its folder's and its project's. */
@Composable
private fun talkHints(session: RuntimeSession, folder: String?): List<String> = listOfNotNull(
    agentLabel(session.agent),
    session.name,
    folder?.trimEnd('/', '\\')?.substringAfterLast('/')?.substringAfterLast('\\'),
    session.projectName
).filter { it.isNotBlank() }.distinct()

/**
 * The talk mode of [chat], whose time moves on while it is on, and which ends when the chat view
 * goes away or the chat stops taking messages. It sends through [submit], and [keep] takes the
 * words that it did not send.
 */
@Composable
internal fun rememberChatTalk(
    chat: TerminalChat,
    session: RuntimeSession,
    view: ChatView,
    submit: (String) -> Boolean,
    keep: (String) -> Unit
): ChatTalk {
    val context = LocalContext.current
    val talk = remember(chat) { ChatTalk(context, submit, keep, asking = { chat.view.value.question != null }) }
    val hints = talkHints(session, view.facts.folder)
    SideEffect { talk.hints = hints }
    LifecycleResumeEffect(talk) { onPauseOrDispose { talk.end() } }
    val writable = view.canWrite
    LaunchedEffect(talk, writable) { if (!writable) talk.end() }
    LaunchedEffect(talk, talk.on) {
        while (talk.on) {
            delay(TICK_MILLIS)
            talk.tick()
        }
    }
    return talk
}

/** Starts talk mode, asking first for the microphone when Kodosi may not use it yet. */
@Composable
internal fun rememberTalkStart(talk: ChatTalk): () -> Unit {
    val microphone = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) talk.start() else talk.note = TalkNote.MICROPHONE
    }
    return {
        when {
            !talk.recognizes -> talk.note = TalkNote.UNAVAILABLE
            talk.permitted -> talk.start()
            else -> microphone.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
}
