package com.kodosi.mobile.terminals

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotFocused
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.pressKey
import androidx.test.platform.app.InstrumentationRegistry
import com.kodosi.mobile.ui.KodosiTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class FirstPromptTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun aFirstPromptStaysOneLineAndEnterOrDoneFinishesIt() {
        val prompt = TextFieldState()
        val claude = AgentInfo(
            "claude",
            "claude",
            installed = true,
            permissions = emptyList(),
            outputs = emptyList(),
            defaultPermissions = AgentPermissions.ASK,
            defaultOutput = AgentOutput.STANDARD
        )
        InstrumentationRegistry.getInstrumentation().setInTouchMode(true)
        Robolectric.buildActivity(ComponentActivity::class.java).setup().get().setContent {
            KodosiTheme { FirstPrompt(claude, prompt, enabled = true, look = TerminalLook.DARK) }
        }
        val field = compose.onNodeWithContentDescription("First prompt")

        field.performTextInput("Fix the sign-in\r\nand its tests")
        field.assertIsFocused()
        field.performKeyInput { pressKey(Key.Enter) }

        assertEquals("Fix the sign-in  and its tests", prompt.text.toString())
        field.assertIsNotFocused()

        field.performClick().assertIsFocused()
        field.performImeAction()
        field.assertIsNotFocused()
    }
}
