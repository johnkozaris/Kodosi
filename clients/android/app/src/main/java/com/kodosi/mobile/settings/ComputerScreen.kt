package com.kodosi.mobile.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.IconTile
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListButton
import com.kodosi.mobile.ui.ListGroup
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.TileTint
import kotlinx.coroutines.launch

private class AwakeChoice(val value: String, @StringRes val title: Int, val symbol: AppSymbol, val tint: Color)

private val awakeChoices = listOf(
    AwakeChoice("agents", R.string.keep_awake_agents, AppSymbol.SPARKLE, TileTint.blue),
    AwakeChoice("always", R.string.keep_awake_always, AppSymbol.SUN, TileTint.orange),
    AwakeChoice("never", R.string.keep_awake_never, AppSymbol.MOON, TileTint.purple)
)

@Composable
fun ComputerScreen(model: AppModel, id: String, back: () -> Unit) {
    val account by model.account.state.collectAsStateWithLifecycle()
    val colors = KodosiTheme.colors
    val scope = rememberCoroutineScope()
    val device = account.devices.firstOrNull { it.deviceId == id }
    val power = account.power[id]
    val online = account.computers.any { it.deviceId == id }
    var sleeping by remember { mutableStateOf(false) }
    var testing by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf(false) }
    LaunchedEffect(device == null) {
        if (device == null) back()
    }
    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally) {
        PageHeader(device?.label.orEmpty(), back)
        if (device == null) return@Column
        LazyColumn(
            Modifier.weight(1f).widthIn(max = 760.dp).fillMaxWidth(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(22.dp)
        ) {
            item {
                Column(
                    Modifier.fillMaxWidth(),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    IconTile(AppSymbol.COMPUTER, TileTint.graphite, 56.dp)
                    Text(device.label, style = KodosiType.title, color = colors.ink, textAlign = TextAlign.Center)
                    val status = listOfNotNull(
                        stringResource(R.string.offline).takeUnless { online },
                        power?.problem ?: power?.note
                    ).joinToString(". ")
                    if (status.isNotEmpty()) {
                        Text(status, style = KodosiType.footnote, color = colors.inkMuted, textAlign = TextAlign.Center)
                    }
                    if (power?.busy != null) {
                        Text(
                            stringResource(if (power.waking) R.string.waking else R.string.testing_wake),
                            style = KodosiType.footnote,
                            color = colors.inkMuted
                        )
                    } else if (power?.canWake == true) {
                        KodosiButton(stringResource(R.string.wake_computer), { model.account.wakeComputer(id) })
                    }
                }
            }
            if (power != null) {
                item {
                    ListGroup(
                        title = stringResource(R.string.stay_awake),
                        footer = when {
                            !online -> stringResource(R.string.change_while_online)
                            power.keepAwake == "always" -> stringResource(R.string.computer_awake_always, device.label)
                            power.keepAwake == "never" -> stringResource(R.string.computer_awake_never, device.label)
                            else -> stringResource(R.string.computer_awake_agents, device.label)
                        }
                    ) {
                        awakeChoices.forEach { choice ->
                            val chosen = power.keepAwake == choice.value
                            ListButton(
                                {
                                    if (!chosen) model.account.keepAwake(id, choice.value)
                                },
                                Modifier.semantics { selected = chosen },
                                enabled = online && power.busy == null
                            ) {
                                ListLine(
                                    stringResource(choice.title),
                                    leading = { IconTile(choice.symbol, choice.tint) }
                                ) {
                                    if (chosen) {
                                        AppIcon(AppSymbol.CHECK, Modifier.size(14.dp), tint = colors.accentStrong)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if (online && power?.busy == null) {
                item {
                    ListGroup {
                        if (power?.canTest == true) {
                            ListButton({ testing = true }) {
                                ListLine(
                                    stringResource(R.string.test_wake),
                                    leading = { IconTile(AppSymbol.CLOCK, TileTint.teal) }
                                )
                            }
                        }
                        ListButton({ sleeping = true }) {
                            ListLine(
                                stringResource(R.string.sleep_computer),
                                leading = { IconTile(AppSymbol.POWER, TileTint.graphite) }
                            )
                        }
                    }
                }
            }
            if (device.deviceId != account.selfDeviceId) {
                item {
                    KodosiTextButton(onClick = { removing = true }, Modifier.padding(start = 4.dp)) {
                        Text(stringResource(R.string.remove), color = colors.danger)
                    }
                }
            }
        }
    }
    if (device == null) return
    if (sleeping) {
        ConfirmDialog(
            stringResource(R.string.sleep_computer_title, device.label),
            listOfNotNull(stringResource(R.string.sleep_computer_message), power?.sleepWarning).joinToString("\n\n"),
            stringResource(R.string.sleep_computer),
            { sleeping = false },
            {
                sleeping = false
                scope.launch { model.runAction { model.account.sleepComputer(device) } }
            }
        )
    }
    if (testing) {
        ConfirmDialog(
            stringResource(R.string.test_wake_title, device.label),
            stringResource(R.string.test_wake_message),
            stringResource(R.string.test_wake_action),
            { testing = false },
            {
                testing = false
                model.account.testWake(device)
            }
        )
    }
    if (removing) {
        ConfirmDialog(
            stringResource(R.string.remove),
            stringResource(R.string.remove_device_message),
            stringResource(R.string.remove),
            { removing = false },
            {
                scope.launch {
                    model.runAction {
                        model.account.revokeDevice(device)
                        removing = false
                    }
                }
            },
            destructive = true
        )
    }
}
