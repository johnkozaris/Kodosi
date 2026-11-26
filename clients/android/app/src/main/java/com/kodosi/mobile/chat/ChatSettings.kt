package com.kodosi.mobile.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiSpinner
import com.kodosi.mobile.ui.KodosiSwitch
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.Radius

/**
 * The agent's own settings, as its settings view in the terminal lists them. The agent checks and
 * saves each change, and says why when it does not take one.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ChatSettingsSheet(chat: TerminalChat, settings: List<ChatSetting>, dismiss: () -> Unit) {
    val colors = KodosiTheme.colors
    val finding = rememberTextFieldState()
    LaunchedEffect(chat) { chat.askSettings() }
    val words = finding.text.toString().trim()
    val shown = if (words.isEmpty()) {
        settings
    } else {
        settings.filter {
            it.label.contains(words, ignoreCase = true) ||
                it.description.contains(words, ignoreCase = true)
        }
    }
    ModalBottomSheet(
        onDismissRequest = dismiss,
        sheetState = rememberModalBottomSheetState(),
        sheetMaxWidth = 600.dp,
        shape = RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet),
        containerColor = colors.surface,
        tonalElevation = 0.dp
    ) {
        Column(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Text(
                stringResource(R.string.settings),
                style = KodosiType.headline,
                color = colors.ink,
                modifier = Modifier.semantics { heading() }
            )
            KodosiField(
                finding,
                placeholder = stringResource(R.string.chat_find_setting),
                lineLimits = TextFieldLineLimits.SingleLine,
                compact = true,
                modifier = Modifier.fillMaxWidth(),
                leading = { AppIcon(AppSymbol.SEARCH, Modifier.size(16.dp), tint = colors.inkMuted) }
            )
            when {
                settings.isEmpty() -> Box(Modifier.fillMaxWidth().heightIn(min = 140.dp), Alignment.Center) {
                    KodosiSpinner()
                }

                shown.isEmpty() -> Text(
                    stringResource(R.string.chat_no_setting_matches),
                    style = KodosiType.footnote,
                    color = colors.inkFaint,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp)
                )

                else -> LazyColumn(Modifier.fillMaxWidth().heightIn(max = 560.dp)) {
                    items(shown, key = { it.key }) { setting -> SettingRow(setting) { chat.set(setting, it) } }
                }
            }
        }
    }
}

@Composable
private fun SettingRow(setting: ChatSetting, set: (String) -> Unit) {
    val colors = KodosiTheme.colors
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(setting.label, style = KodosiType.body, color = colors.ink)
                    if (setting.locked) {
                        val locked = stringResource(R.string.chat_setting_locked)
                        Spacer(Modifier.width(5.dp))
                        AppIcon(
                            AppSymbol.LOCK,
                            Modifier.size(12.dp).semantics { contentDescription = locked },
                            tint = colors.inkFaint
                        )
                    }
                }
                if (setting.description.isNotEmpty()) {
                    Text(setting.description, style = KodosiType.footnote, color = colors.inkMuted)
                }
            }
            Spacer(Modifier.width(10.dp))
            SettingControl(setting, set)
        }
        setting.refusal?.let { Text(it, style = KodosiType.caption, color = colors.caution) }
    }
}

@Composable
private fun SettingControl(setting: ChatSetting, set: (String) -> Unit) {
    when (setting.kind) {
        ChatSettingKind.TOGGLE -> KodosiSwitch(
            setting.value == "true",
            { set(if (it) "true" else "false") },
            setting.label,
            enabled = !setting.locked
        )

        ChatSettingKind.CHOICE -> {
            var open by remember { mutableStateOf(false) }
            Box {
                KodosiButton(
                    { open = true },
                    Modifier.semantics { contentDescription = setting.label },
                    ButtonKind.SECONDARY,
                    ButtonSize.SMALL,
                    !setting.locked
                ) {
                    Text(setting.value, style = KodosiType.callout, maxLines = 1)
                    Spacer(Modifier.width(6.dp))
                    AppIcon(AppSymbol.CHEVRON_DOWN, Modifier.size(10.dp), tint = KodosiTheme.colors.inkFaint)
                }
                KodosiMenu(open, { open = false }) {
                    setting.options.forEach { option ->
                        KodosiMenuItem(text = { Text(option) }, onClick = {
                            open = false
                            if (option != setting.value) set(option)
                        })
                    }
                }
            }
        }

        ChatSettingKind.TEXT, ChatSettingKind.NUMBER -> {
            val draft = rememberTextFieldState(setting.value)
            LaunchedEffect(setting.value) { draft.setTextAndPlaceCursorAtEnd(setting.value) }
            KodosiField(
                draft,
                lineLimits = TextFieldLineLimits.SingleLine,
                compact = true,
                enabled = !setting.locked,
                keyboardOptions = KeyboardOptions(
                    keyboardType = if (setting.kind ==
                        ChatSettingKind.NUMBER
                    ) {
                        KeyboardType.Decimal
                    } else {
                        KeyboardType.Text
                    },
                    imeAction = ImeAction.Done
                ),
                onKeyboardAction = {
                    val value = draft.text.toString()
                    if (value != setting.value) set(value)
                },
                modifier = Modifier.width(150.dp).semantics { contentDescription = setting.label }
            )
        }
    }
}
