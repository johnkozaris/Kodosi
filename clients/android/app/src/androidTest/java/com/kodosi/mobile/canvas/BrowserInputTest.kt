package com.kodosi.mobile.canvas

import android.view.KeyCharacterMap
import android.view.KeyEvent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.kodosi.mobile.AndroidWords
import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.Ffi
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.string
import java.util.Base64
import java.util.concurrent.FutureTask
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BrowserInputTest {
    @Test
    fun hardwareDeadAccentComposesWithTheNextCharacterAndResets() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val result = FutureTask<Unit> {
            val words = AndroidWords(instrumentation.targetContext)
            val received = mutableListOf<String>()
            val commands = CommandSink(words).apply {
                transport = { bytes ->
                    val command = RuntimeJson.parseToJsonElement(bytes.decodeToString()).jsonObject
                    when (command.string("type")) {
                        "document.open" -> receive(
                            RuntimeEvent(
                                "document.result",
                                null,
                                0,
                                fields(
                                    "operation" to "document.open",
                                    "requestId" to command.string("requestId")
                                )
                            )
                        )

                        "document.send" -> {
                            val frame = Base64.getDecoder().decode(command.string("data"))
                            val message = RuntimeJson.parseToJsonElement(
                                frame.copyOfRange(1, frame.size).decodeToString()
                            ).jsonObject
                            if (message.string("type") == "input") {
                                received += requireNotNull(message.getValue("input").jsonObject.string("text"))
                            }
                        }
                    }
                    Ffi.OK
                }
            }
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            try {
                BrowserFeed(CanvasStore(commands, words), "fixture", scope).use { feed ->
                    feed.show(true)
                    val view = BrowserSurface(instrumentation.targetContext, feed)
                    assertTrue(
                        view.onKeyDown(
                            KeyEvent.KEYCODE_UNKNOWN,
                            character(KeyCharacterMap.COMBINING_ACCENT or 0x00b4)
                        )
                    )
                    assertTrue(received.isEmpty())
                    assertTrue(view.onKeyDown(KeyEvent.KEYCODE_UNKNOWN, character('e'.code)))
                    assertTrue(view.onKeyDown(KeyEvent.KEYCODE_UNKNOWN, character('e'.code)))
                    assertEquals(listOf("\u00e9", "e"), received)
                }
            } finally {
                scope.cancel()
            }
        }
        instrumentation.runOnMainSync(result)
        result.get()
    }

    private fun character(codePoint: Int) = object : KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_UNKNOWN) {
        override fun getUnicodeChar(metaState: Int): Int = codePoint
    }
}
