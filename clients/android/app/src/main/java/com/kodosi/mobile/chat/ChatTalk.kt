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
import androidx.lifecycle.compose.LifecycleStartEffect
import com.kodosi.mobile.terminals.RuntimeSession
import com.kodosi.mobile.terminals.agentLabel
import java.util.Locale
import kotlinx.coroutines.delay

private const val TICK_MILLIS = 100L
private const val MODEL_CHECK_MILLIS = 2_000L

// Google's recognizer keeps a session going for at most five minutes; talk mode starts the next one.
private const val SESSION_MILLIS = 300_000

// A recognizer that just ended a session can refuse the next one for a moment.
private const val RETRY_MILLIS = 250L
private const val RETRIES = 3

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

    /** This device turns speech into text itself; without that, the chat shows no talk key. */
    val recognizes: Boolean = SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

    val permitted: Boolean
        get() = context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    private var turn = TalkTurn(0L)
    private var recognizer: SpeechRecognizer? = null
    private var language = ""
    private var ready = false

    /** The recognizer heard words in this talk mode, so it worked, and a stop keeps them quietly. */
    private var heard = false

    /** Talk mode asked for the language's model once after the recognizer said it does not have it. */
    private var fetched = false

    /** The recognizer gave this session's speech in segments, so its results only repeat them. */
    private var segmented = false

    /** When talk mode starts the recognizer again after it refused, and how often it did since words came. */
    private var retryAt: Long? = null
    private var retries = 0

    /** When talk mode last asked whether its language is installed, while it waits for the download. */
    private var modelCheckedAt: Long? = null

    fun start() {
        if (on) return
        val starting = newRecognizer()
        on = true
        listening = false
        heard = false
        fetched = false
        note = null
        turn = TalkTurn(SystemClock.uptimeMillis())
        publish()
        val locales = context.getSystemService(LocaleManager::class.java).systemLocales
        val wanted = (0 until locales.size()).map { locales[it].stripExtensions() }
            .ifEmpty { listOf(Locale.getDefault().stripExtensions()) }
        language = wanted.first().toLanguageTag()
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
        val retry = retryAt
        if (retry != null && now >= retry) {
            retryAt = null
            newRecognizer()
            listen()
        }
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

    /**
     * Listens in the first of the person's languages that the recognizer has or can download, so a
     * language that the person put first wins over one that is installed already.
     */
    private fun prepare(wanted: List<Locale>, support: RecognitionSupport) {
        val downloadable = support.pendingOnDeviceLanguages + support.supportedOnDeviceLanguages
        for (locale in wanted) {
            closestLanguage(locale, support.installedOnDeviceLanguages)?.let {
                language = it
                listen()
                return
            }
            closestLanguage(locale, downloadable)?.let {
                language = it
                download()
                return
            }
        }
        val told = support.installedOnDeviceLanguages.isNotEmpty() || downloadable.isNotEmpty()
        if (told) stop(SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED) else listen()
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
        segmented = false
        recognizer?.startListening(intent())
    }

    private fun intent(): Intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
        .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
        .putExtra(RecognizerIntent.EXTRA_ENABLE_FORMATTING, RecognizerIntent.FORMATTING_OPTIMIZE_QUALITY)
        .putExtra(RecognizerIntent.EXTRA_HIDE_PARTIAL_TRAILING_PUNCTUATION, true)
        // The agent gets the person's words as they said them.
        .putExtra(RecognizerIntent.EXTRA_MASK_OFFENSIVE_WORDS, false)
        .putStringArrayListExtra(RecognizerIntent.EXTRA_BIASING_STRINGS, ArrayList(hints))
        // A session lasts as long as the recognizer allows, whatever the pauses in it, and gives the
        // speech in segments, as Chrome's continuous recognition asks for it.
        .putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, RecognizerIntent.EXTRA_SPEECH_INPUT_MINIMUM_LENGTH_MILLIS)
        .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_MINIMUM_LENGTH_MILLIS, SESSION_MILLIS)

    /** A recognizer of talk mode's own, in place of the one before. */
    private fun newRecognizer(): SpeechRecognizer {
        recognizer?.destroy()
        return SpeechRecognizer.createOnDeviceSpeechRecognizer(context).also {
            it.setRecognitionListener(Hearing(it))
            recognizer = it
        }
    }

    private fun failed(error: Int) {
        val now = SystemClock.uptimeMillis()
        val silent = error == SpeechRecognizer.ERROR_NO_MATCH || error == SpeechRecognizer.ERROR_SPEECH_TIMEOUT
        val refused = error == SpeechRecognizer.ERROR_RECOGNIZER_BUSY || error == SpeechRecognizer.ERROR_CLIENT ||
            error == SpeechRecognizer.ERROR_SERVER_DISCONNECTED
        when {
            silent && ready -> {
                apply(turn.stopping(now, asking()))
                listen()
            }

            // A new recognizer gets over a refused session or a lost recognition service.
            refused && listening && retries < RETRIES -> {
                retries++
                retryAt = now + RETRY_MILLIS
                apply(turn.stopping(now, asking()))
            }

            // Google's recognizer can list no languages until it downloaded one, and then says that
            // it lacks the model only once it listens.
            error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE && !heard && !fetched -> {
                fetched = true
                listening = false
                download()
            }

            else -> stop(error)
        }
    }

    /** Talk mode stops: with a note while it heard no words, else quietly with the words kept. */
    private fun stop(error: Int) {
        Log.w(TAG, "Talk mode stopped on speech recognition error $error.")
        finish(
            when {
                heard -> null

                error == SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> TalkNote.MICROPHONE

                error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED ||
                    error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> TalkNote.UNAVAILABLE

                else -> TalkNote.FAILED
            },
            turn.takeUnsent()
        )
    }

    /** Words came, so the recognizer works: a later stop keeps them quietly, and refusals get new retries. */
    private fun wordsCame() {
        heard = true
        retries = 0
    }

    private fun finish(why: TalkNote?, unsent: String) {
        recognizer?.destroy()
        recognizer = null
        modelCheckedAt = null
        retryAt = null
        retries = 0
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
            if (!current) return
            val words = partialResults.partialWords()
            if (words.isNotBlank()) wordsCame()
            apply(turn.hearing(words, SystemClock.uptimeMillis(), asking()))
        }

        override fun onSegmentResults(segmentResults: Bundle) {
            if (!current) return
            segmented = true
            val words = segmentResults.finalWords()
            if (words.isNotBlank()) wordsCame()
            apply(turn.settling(words, SystemClock.uptimeMillis(), asking()))
        }

        override fun onResults(results: Bundle?) {
            if (!current) return
            val now = SystemClock.uptimeMillis()
            val words = results.finalWords()
            if (words.isNotBlank()) wordsCame()
            // After segments, the results repeat the speech of the whole session.
            apply(if (segmented) turn.stopping(now, asking()) else turn.settling(words, now, asking()))
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

/**
 * Names that the person may say, as they say them: the agent's, the terminal's, its folder's and its
 * project's, where "talk-in_chat" is "talk in chat".
 */
@Composable
private fun talkHints(session: RuntimeSession, folder: String?): List<String> = listOfNotNull(
    agentLabel(session.agent),
    session.name,
    folder?.trimEnd('/', '\\')?.substringAfterLast('/')?.substringAfterLast('\\'),
    session.projectName
).map { it.replace(NAME_SEPARATORS, " ").trim() }.filter { it.isNotEmpty() }.distinct()

private val NAME_SEPARATORS = Regex("""[-_./\\]+""")

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
    // A system dialog over the chat, such as the one that downloads the language, leaves talk mode
    // on, and talk mode keeps the chat linked meanwhile; leaving the app ends it.
    LifecycleStartEffect(talk) { onStopOrDispose { talk.end() } }
    LaunchedEffect(talk, talk.on) { if (talk.on) chat.watch() }
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
            talk.permitted -> talk.start()
            else -> microphone.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
}
