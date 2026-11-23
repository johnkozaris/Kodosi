package com.kodosi.mobile.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class KodosiFieldTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun aByteLimitKeepsTheWholeCharactersThatFit() {
        val note = TextFieldState()
        show { KodosiField(note, label = "Note", inputTransformation = Utf8Limit(6)) }
        val field = compose.onNodeWithContentDescription("Note")

        field.performTextInput("ab€\uD83D\uDE00")
        assertEquals("ab€", note.text.toString())

        field.performTextInput("d")
        assertEquals("ab€d", note.text.toString())
    }

    private fun show(content: @Composable () -> Unit) {
        Robolectric.buildActivity(ComponentActivity::class.java).setup().get().setContent { KodosiTheme { content() } }
    }
}
