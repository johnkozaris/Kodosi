package com.kodosi.mobile.chat

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.view.textclassifier.TextClassificationManager
import android.view.textclassifier.TextLanguage
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.kodosi.mobile.ui.InlineMarkdown
import java.util.Locale
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Reads a message aloud in the phone's voice for the message's language, without the signs of its
 * Markdown. Reading another message stops the first.
 */
internal object MessageReader {
    var reading by mutableStateOf<String?>(null)
        private set
    private val main = Handler(Looper.getMainLooper())
    private var voice: TextToSpeech? = null
    private var ready = CompletableDeferred<Boolean>()
    private var said = 0

    suspend fun read(context: Context, text: String) {
        said += 1
        val id = said
        val words = spoken(text)
        val language = withContext(Dispatchers.Default) { language(context, words) }
        val voice = voice(context)
        if (!ready.await()) {
            voice.shutdown()
            this.voice = null
            ready = CompletableDeferred()
            return
        }
        if (id != said) return
        voice.language = language?.takeIf { voice.isLanguageAvailable(it) >= TextToSpeech.LANG_AVAILABLE }
            ?: Locale.getDefault()
        reading = text
        voice.speak(words, TextToSpeech.QUEUE_FLUSH, null, id.toString())
    }

    fun stop() {
        said += 1
        reading = null
        voice?.stop()
    }

    private fun voice(context: Context): TextToSpeech = voice ?: TextToSpeech(context.applicationContext) { status ->
        ready.complete(status == TextToSpeech.SUCCESS)
    }.also { created ->
        created.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) = Unit

            override fun onDone(utteranceId: String?) = ended(utteranceId)

            override fun onStop(utteranceId: String?, interrupted: Boolean) = ended(utteranceId)

            override fun onError(utteranceId: String?, errorCode: Int) = ended(utteranceId)

            @Deprecated("Deprecated in Java")
            override fun onError(utteranceId: String?) = ended(utteranceId)
        })
        voice = created
    }

    private fun ended(utteranceId: String?) {
        main.post {
            if (utteranceId == said.toString()) reading = null
        }
    }

    private fun language(context: Context, words: String): Locale? {
        val classifier = context.getSystemService(TextClassificationManager::class.java)?.textClassifier ?: return null
        val found = classifier.detectLanguage(TextLanguage.Request.Builder(words.take(MOST_DETECTED)).build())
        if (found.localeHypothesisCount == 0) return null
        val likeliest = found.getLocale(0)
        return likeliest.toLocale().takeIf { found.getConfidenceScore(likeliest) >= SURE }
    }

    private fun spoken(text: String): String = ProseBlock.blocks(text).joinToString("\n\n") { block ->
        when (block) {
            is ProseBlock.Lines -> block.text.lines().joinToString("\n") { plain(it.trimStart(' ').removePrefix("• ")) }

            is ProseBlock.Heading -> plain(block.text)

            is ProseBlock.Code -> block.text

            is ProseBlock.Table -> (listOf(block.table.header) + block.table.rows).filter { it.isNotEmpty() }
                .joinToString("\n") { row -> row.joinToString(", ") { plain(it) } }
        }
    }

    private fun plain(line: String) = InlineMarkdown.runs(line).joinToString("") { it.text }

    private const val MOST_DETECTED = 2000
    private const val SURE = 0.5f
}
