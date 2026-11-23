package com.kodosi.mobile.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.text.input.ImeAction
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
    fun aOneLineFieldGivesEnterAndItsKeyboardActionToTheCaller() {
        val key = TextFieldState()
        var actions = 0
        show {
            KodosiField(
                key,
                placeholder = "Recovery key",
                lineLimits = TextFieldLineLimits.SingleLine,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                onKeyboardAction = { actions++ }
            )
        }
        val field = compose.onNodeWithContentDescription("Recovery key")

        field.performTextInput("ABCD-EFGH")
        field.performKeyInput { pressKey(Key.Enter) }
        field.performImeAction()

        assertEquals("ABCD-EFGH", key.text.toString())
        assertEquals(2, actions)
    }

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
