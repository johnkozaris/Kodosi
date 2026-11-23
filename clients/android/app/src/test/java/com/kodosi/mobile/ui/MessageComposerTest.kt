package com.kodosi.mobile.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.pressKey
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class MessageComposerTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun enterStartsANewLineAndOnlySendSends() {
        val draft = TextFieldState()
        val sent = mutableListOf<String>()
        Robolectric.buildActivity(ComponentActivity::class.java).setup().get().setContent {
            KodosiTheme { MessageComposer(draft, busy = false, send = { sent += draft.text.toString() }) }
        }
        val field = compose.onNodeWithContentDescription("Write a message")
        val send = compose.onNodeWithContentDescription("Send")

        field.performTextInput("   ")
        send.assertIsNotEnabled()
        draft.clearText()
        field.performTextInput("git status")
        field.performKeyInput { pressKey(Key.Enter) }
        field.performTextInput("ls")

        assertEquals("git status\nls", draft.text.toString())
        assertEquals(emptyList<String>(), sent)
        send.assertIsEnabled().performClick()
        assertEquals(listOf("git status\nls"), sent)
    }
}
