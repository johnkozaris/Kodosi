package com.kodosi.mobile.dosi

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.flag
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.runtime.strings
import com.kodosi.mobile.ui.ActionIcon
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.Elevation
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiMenu
import com.kodosi.mobile.ui.KodosiMenuItem
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.MessageComposer
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.Radius
import com.kodosi.mobile.ui.SelectableWords
import com.kodosi.mobile.ui.Unfolding
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.raised
import com.kodosi.mobile.ui.snappy
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

@Composable
fun DosiScreen(model: AppModel, target: (String, String) -> Unit) {
    val state by model.dosi.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val computers = account.computers.map { DosiComputer(it.deviceId, it.label) }.toMutableList()
    state.snapshot?.last?.let { last ->
        if (computers.none { it.device == last.device } &&
            account.devices.any { it.deviceId == last.device && !it.recoveryKey }
        ) {
            computers.add(last)
        }
    }
    var picked by rememberSaveable { mutableStateOf(model.dosi.device) }
    val computer = computers.firstOrNull { it.device == picked }
        ?: computers.firstOrNull { it.device == state.snapshot?.last?.device } ?: computers.firstOrNull()
    var choosing by remember { mutableStateOf(false) }
    var renewing by remember { mutableStateOf(false) }
    val draft = rememberTextFieldState(model.dosi.draft)
    LaunchedEffect(draft) { snapshotFlow { draft.text.toString() }.collect { model.dosi.draft = it } }
    var sending by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val scroll = rememberLazyListState()
    var visible by remember { mutableStateOf(false) }
    LifecycleResumeEffect(Unit) {
        visible = true
        model.dosi.see()
        onPauseOrDispose { visible = false }
    }
    val rows = remember(state.items) { dosiRows(state.items) }
    LaunchedEffect(state.items.lastOrNull()?.id, state.busy, visible) {
        if (visible) model.dosi.see()
        if (rows.isNotEmpty() &&
            (scroll.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: Int.MAX_VALUE) >= rows.size - 4
        ) {
            scroll.scrollToItem(rows.size)
        }
    }
    Column(Modifier.fillMaxSize().imePadding()) {
        PageHeader(stringResource(R.string.dosi), actions = {
            if (state.busy) ActionIcon(AppSymbol.STOP, R.string.stop, model.dosi::stop)
            ActionIcon(AppSymbol.COMPOSE, R.string.new_chat, { renewing = true }, enabled = state.connected)
        })
        LazyColumn(
            state = scroll,
            modifier = Modifier.weight(1f),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp)
        ) {
            item {
                if (state.earlier) {
                    KodosiTextButton(onClick = { scope.launch { model.runAction { model.dosi.earlier() } } }) {
                        Text(stringResource(R.string.load_older))
                    }
                }
            }
            if (state.snapshot != null && state.items.isEmpty()) {
                item(key = "empty") {
                    Box(Modifier.animateItem(fade(), snappy(), fade())) {
                        EmptyState(
                            stringResource(if (computer == null) R.string.no_computer else R.string.ask_dosi),
                            stringResource(
                                if (computer ==
                                    null
                                ) {
                                    R.string.no_computer_message
                                } else {
                                    R.string.dosi_empty_message
                                }
                            ),
                            AppSymbol.DOSI
                        )
                    }
                }
            }
            items(rows, key = { it.key }) { row ->
                Column(Modifier.animateItem(fade(), snappy(), fade())) {
                    if (row.steps) DosiSteps(row, target) else DosiItemView(model, row.items.first(), target)
                    row.items.forEach { item ->
                        state.snapshot?.asks?.firstOrNull { it.string("item") == item.id }?.let {
                            Row {
                                Text(
                                    stringResource(R.string.queued_message),
                                    style = KodosiType.footnote,
                                    modifier = Modifier.weight(1f)
                                )
                                KodosiTextButton(onClick = {
                                    model.dosi.takeBack(item.id)
                                }) { Text(stringResource(R.string.take_back)) }
                            }
                        }
                    }
                }
            }
            if (state.busy &&
                state.items.lastOrNull()?.body?.flag("live") != true
            ) {
                item(key = "thinking") {
                    ProgressCaption(stringResource(R.string.thinking), Modifier.animateItem(fade(), snappy(), fade()))
                }
            }
            items(state.snapshot?.queued.orEmpty(), key = { it.string("id").orEmpty() }) { queued ->
                Row(Modifier.animateItem(fade(), snappy(), fade())) {
                    Text(queued.string("text").orEmpty(), modifier = Modifier.weight(1f))
                    KodosiTextButton(onClick = {
                        queued.string("id")?.let(model.dosi::takeBack)
                    }) { Text(stringResource(R.string.take_back)) }
                }
            }
        }
        computer?.let { chosen ->
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp)) {
                KodosiTextButton(onClick = { choosing = true }) {
                    Text(
                        chosen.name +
                            if (account.onlineHosts.contains(chosen.device)) {
                                ""
                            } else {
                                " · " +
                                    stringResource(R.string.not_connected)
                            }
                    )
                }
                KodosiMenu(choosing, onDismissRequest = { choosing = false }) {
                    computers.forEach { option ->
                        KodosiMenuItem(text = { Text(option.name) }, onClick = {
                            picked = option.device
                            model.dosi.device = option.device
                            choosing = false
                        })
                    }
                }
            }
        }
        Unfolding(failure) { ErrorNote(it, Modifier.padding(horizontal = 12.dp)) }
        MessageComposer(draft, sending, enabled = computer != null && state.connected, send = {
            val chosen = computer ?: return@MessageComposer
            val sent = draft.text.toString()
            sending = true
            failure = null
            scope.launch {
                try {
                    model.dosi.send(sent, chosen)
                    if (draft.text.toString() == sent) draft.clearText()
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    failure = error.localizedMessage
                } finally {
                    sending = false
                }
            }
        })
    }
    if (renewing) {
        ConfirmDialog(
            stringResource(R.string.new_chat_title),
            stringResource(R.string.new_chat_message),
            stringResource(R.string.new_chat),
            { renewing = false },
            {
                scope.launch {
                    model.runAction {
                        model.dosi.renew()
                        renewing = false
                    }
                }
            }
        )
    }
}

/** Several steps in a row fold into one line, as on the iPhone; a tap shows each of them. */
@Composable
private fun DosiSteps(row: DosiRow, open: (String, String) -> Unit) {
    if (row.items.size == 1) {
        DosiStep(row.items.first(), open)
        return
    }
    var shown by remember(row.key) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        KodosiTextButton(
            onClick = { shown = !shown },
            modifier = Modifier.background(KodosiTheme.colors.sheet, RoundedCornerShape(50))
        ) {
            AppIcon(
                if (row.items.any { it.body.string("status") == "running" }) AppSymbol.DOSI else AppSymbol.CHECK,
                Modifier.size(12.dp)
            )
            Text(row.line, modifier = Modifier.padding(start = 7.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (shown) row.items.forEach { DosiStep(it, open) }
    }
}

@Composable
private fun DosiStep(item: DosiItem, open: (String, String) -> Unit) {
    val body = item.body
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        val target = body["target"] as? JsonObject
        KodosiTextButton(onClick = {
            target?.string("kind")?.let { targetKind ->
                target.string("value")?.let { open(targetKind, it) }
            }
        }, modifier = Modifier.background(KodosiTheme.colors.sheet, RoundedCornerShape(50))) {
            AppIcon(
                if (body.string("status") == "running") AppSymbol.DOSI else AppSymbol.CHECK,
                Modifier.size(12.dp)
            )
            Text(item.stepName, modifier = Modifier.padding(start = 7.dp))
        }
        body.string("output")?.takeIf { it.isNotBlank() }?.let { output ->
            var expanded by remember { mutableStateOf(false) }
            KodosiTextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.result)) }
            if (expanded) SelectableWords(output)
        }
    }
}

@Composable
private fun DosiItemView(model: AppModel, item: DosiItem, open: (String, String) -> Unit) {
    val colors = KodosiTheme.colors
    val body = item.body
    val kind = body.string("kind")
    when (kind) {
        "person" -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            Spacer(Modifier.width(48.dp))
            Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
                SelectableWords(
                    body.string("text").orEmpty(),
                    Modifier.background(
                        colors.accentSoft,
                        RoundedCornerShape(topStart = 14.dp, topEnd = 14.dp, bottomStart = 14.dp, bottomEnd = 4.dp)
                    )
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    markdown = true
                )
            }
        }

        "text", "note", "notice" -> body.string("text")?.let { SelectableWords(it, markdown = true) }

        "reasoning" -> {
            var expanded by remember { mutableStateOf(false) }
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                KodosiTextButton(onClick = { expanded = !expanded }) { Text(stringResource(R.string.thinking)) }
                if (expanded) SelectableWords(body.string("text").orEmpty())
            }
        }

        "question" -> {
            val answered = body["answer"]?.toString()?.takeIf { it != "null" } != null
            Column(
                if (answered) {
                    Modifier
                } else {
                    Modifier.fillMaxWidth()
                        .raised(colors, RoundedCornerShape(Radius.lg), elevation = Elevation.LIFTED)
                        .padding(horizontal = 12.dp, vertical = 11.dp)
                },
                verticalArrangement = Arrangement.spacedBy(9.dp)
            ) { DosiQuestion(model, item) }
        }
    }
}

@Composable
private fun DosiQuestion(model: AppModel, item: DosiItem) {
    val body = item.body
    var choices by rememberSaveable(item.id) { mutableStateOf<Set<String>>(emptySet()) }
    val own = rememberSaveable(item.id, saver = TextFieldState.Saver) { TextFieldState() }
    var sending by remember { mutableStateOf(false) }
    val answered = body["answer"]?.toString()?.takeIf { it != "null" } != null
    val scope = rememberCoroutineScope()
    if (answered) {
        Row(
            Modifier.background(
                KodosiTheme.colors.sheet,
                RoundedCornerShape(50)
            ).padding(horizontal = 10.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            AppIcon(AppSymbol.CHECK, Modifier.size(10.dp), tint = KodosiTheme.colors.ready)
            Text(
                body.strings("answer").joinToString(", "),
                style = KodosiType.caption,
                color = KodosiTheme.colors.inkMuted
            )
        }
    } else {
        val answer: (List<String>) -> Unit = { values ->
            sending = true
            scope.launch {
                try {
                    model.runAction { model.dosi.answer(item, values) }
                } finally {
                    sending = false
                }
            }
        }
        Text(body.string("question").orEmpty(), style = KodosiType.subhead, color = KodosiTheme.colors.ink)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            body.strings("options").forEach { option ->
                KodosiButton(
                    option,
                    onClick = {
                        if (body.flag("multiple")) {
                            choices = if (option in choices) choices - option else choices + option
                        } else {
                            answer(listOf(option))
                        }
                    },
                    kind = if (option in choices) ButtonKind.TINTED else ButtonKind.SECONDARY,
                    size = ButtonSize.SMALL,
                    enabled = !sending
                )
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            KodosiField(
                own,
                modifier = Modifier.weight(1f),
                placeholder = stringResource(R.string.your_answer),
                lineLimits = TextFieldLineLimits.SingleLine,
                enabled = !sending,
                compact = true,
                textStyle = KodosiType.footnote
            )
            if (body.flag("multiple") || own.text.isNotBlank()) {
                val values = body.strings("options").filter(choices::contains) +
                    listOfNotNull(own.text.toString().trim().takeIf(String::isNotEmpty))
                KodosiButton(
                    stringResource(R.string.send),
                    { answer(values) },
                    size = ButtonSize.SMALL,
                    enabled = !sending && values.isNotEmpty()
                )
            }
        }
    }
}
