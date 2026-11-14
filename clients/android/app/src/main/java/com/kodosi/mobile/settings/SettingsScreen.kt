package com.kodosi.mobile.settings

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PersistableBundle
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Badge
import androidx.compose.material3.Surface
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppLanguage
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.BuildConfig
import com.kodosi.mobile.R
import com.kodosi.mobile.account.AppLock
import com.kodosi.mobile.account.Device
import com.kodosi.mobile.account.DeviceCode
import com.kodosi.mobile.terminals.TerminalPreview
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ButtonKind
import com.kodosi.mobile.ui.ButtonSize
import com.kodosi.mobile.ui.CodeScanDialog
import com.kodosi.mobile.ui.CodeScanWords
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.CountBadge
import com.kodosi.mobile.ui.IconTile
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiCard
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiSwitch
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListButton
import com.kodosi.mobile.ui.ListGroup
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.LocalLinkOpener
import com.kodosi.mobile.ui.MenuPicker
import com.kodosi.mobile.ui.PageHeader
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.SectionTitle
import com.kodosi.mobile.ui.SegmentOption
import com.kodosi.mobile.ui.SegmentedPill
import com.kodosi.mobile.ui.SelectableWords
import com.kodosi.mobile.ui.SwitchRow
import com.kodosi.mobile.ui.TextEntryDialog
import com.kodosi.mobile.ui.TileTint
import com.kodosi.mobile.ui.canScanCodes
import com.kodosi.mobile.ui.raised
import kotlinx.coroutines.launch

@Composable
fun SettingsScreen(model: AppModel, friends: () -> Unit, licenses: () -> Unit) {
    val account by model.account.state.collectAsStateWithLifecycle()
    val people by model.people.state.collectAsStateWithLifecycle()
    val preferences by model.preferences.state.collectAsStateWithLifecycle()
    val notificationIssue by model.push.issue.collectAsStateWithLifecycle()
    val colors = KodosiTheme.colors
    val context = LocalContext.current
    val activity = context as Activity
    val scope = rememberCoroutineScope()
    val camera = remember(context) { canScanCodes(context) }
    var signOut by remember { mutableStateOf(false) }
    var delete by remember { mutableStateOf(false) }
    var approving by remember { mutableStateOf(false) }
    var checked by remember { mutableStateOf<Pair<String, String>?>(null) }
    var removing by remember { mutableStateOf<Device?>(null) }
    var recovery by remember { mutableStateOf<String?>(null) }
    var creatingRecovery by remember { mutableStateOf(false) }
    var scanning by remember { mutableStateOf(false) }
    val links = LocalLinkOpener.current
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        model.updatePush(force = true)
    }
    LaunchedEffect(account.deletionPage) {
        account.deletionPage?.let {
            links.open(it)
            model.account.openedDeletionPage()
        }
    }
    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally) {
        PageHeader(stringResource(R.string.settings))
        LazyColumn(
            Modifier.weight(1f).widthIn(max = 760.dp).fillMaxWidth(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(22.dp)
        ) {
            item {
                val name = account.displayName ?: stringResource(R.string.you)
                FlowRow(
                    Modifier.fillMaxWidth().raised(colors, RoundedCornerShape(18.dp)).padding(16.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                    itemVerticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp)
                    ) {
                        PersonAvatar(name, own = true, size = 52.dp)
                        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                            Text(name, style = KodosiType.title, color = colors.ink, maxLines = 2)
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(5.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                AppIcon(AppSymbol.LOCK, Modifier.size(11.dp))
                                Text(
                                    stringResource(R.string.encrypted_end_to_end),
                                    style = KodosiType.footnote,
                                    color = colors.inkMuted
                                )
                            }
                        }
                    }
                    KodosiButton(
                        stringResource(R.string.sign_out),
                        { signOut = true },
                        kind = ButtonKind.SECONDARY,
                        size = ButtonSize.SMALL
                    )
                }
            }
            item {
                ListGroup {
                    ListButton(friends) {
                        ListLine(stringResource(R.string.friends), leading = {
                            IconTile(AppSymbol.FRIENDS, TileTint.blue)
                        }) {
                            if (people.incoming.isNotEmpty()) {
                                CountBadge(people.incoming.size)
                            }
                            AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint)
                        }
                    }
                }
            }
            item {
                ListGroup(title = stringResource(R.string.your_devices)) {
                    account.reset?.takeIf { !it.thisDevice }?.let { reset ->
                        ListLine(
                            reset.deviceLabel,
                            subtitle = stringResource(
                                R.string.start_fresh_wait,
                                java.text.DateFormat.getDateTimeInstance().format(java.util.Date(reset.readyAtMs))
                            ),
                            leading = { IconTile(AppSymbol.LOCK, TileTint.amber) }
                        ) {
                            KodosiButton(
                                stringResource(R.string.stop_reset),
                                model.account::stopReset,
                                kind = ButtonKind.SECONDARY,
                                size = ButtonSize.SMALL
                            )
                        }
                    }
                    account.devices.sortedBy {
                        when {
                            it.deviceId == account.selfDeviceId -> 0
                            it.recoveryKey -> 2
                            else -> 1
                        }
                    }.forEach { device ->
                        ListLine(
                            if (device.recoveryKey) stringResource(R.string.recovery_key) else device.label,
                            subtitle = when {
                                device.deviceId == account.selfDeviceId -> stringResource(R.string.this_device)
                                device.recoveryKey -> stringResource(R.string.recovery_message)
                                else -> null
                            },
                            leading = {
                                IconTile(
                                    when {
                                        device.recoveryKey -> AppSymbol.KEY
                                        device.deviceId == account.selfDeviceId -> AppSymbol.PHONE
                                        else -> AppSymbol.COMPUTER
                                    },
                                    if (device.recoveryKey) TileTint.amber else TileTint.graphite
                                )
                            }
                        ) {
                            if (device.deviceId != account.selfDeviceId) {
                                KodosiTextButton(onClick = {
                                    removing = device
                                }) { Text(stringResource(R.string.remove)) }
                            }
                        }
                    }
                    account.requests.forEach { request ->
                        ListLine(
                            request.deviceLabel,
                            subtitle = stringResource(R.string.waiting_for_approval),
                            leading = { IconTile(AppSymbol.PHONE, TileTint.blue) }
                        ) {
                            KodosiButton(
                                stringResource(R.string.approve),
                                { approving = true },
                                size = ButtonSize.SMALL
                            )
                        }
                    }
                    ListButton({ approving = true }) {
                        ListLine(stringResource(R.string.add_device), leading = {
                            IconTile(AppSymbol.ADD, TileTint.blue)
                        }) {
                            AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint)
                        }
                    }
                }
            }
            item {
                ListGroup(
                    title = stringResource(R.string.recovery_key),
                    footer = stringResource(R.string.recovery_message)
                ) {
                    ListButton({ creatingRecovery = true }) {
                        ListLine(stringResource(R.string.create_recovery_key), leading = {
                            IconTile(AppSymbol.KEY, TileTint.amber)
                        }) {
                            AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint)
                        }
                    }
                }
            }
            item {
                TerminalPreview(preferences.textSize)
            }
            item {
                ListGroup(title = stringResource(R.string.terminals)) {
                    ListLine(stringResource(R.string.appearance), leading = {
                        IconTile(AppSymbol.APPEARANCE, TileTint.graphite)
                    }) {
                        SegmentedPill(
                            listOf(
                                SegmentOption(stringResource(R.string.automatic)),
                                SegmentOption(stringResource(R.string.light)),
                                SegmentOption(stringResource(R.string.dark))
                            ),
                            when (preferences.appearance) {
                                "light" -> 1
                                "dark" -> 2
                                else -> 0
                            }
                        ) { index ->
                            model.preferences.update { it.copy(appearance = listOf("system", "light", "dark")[index]) }
                        }
                    }
                    ListLine(stringResource(R.string.size), leading = {
                        IconTile(AppSymbol.TEXT_SIZE, TileTint.teal)
                    }) {
                        SegmentedPill(
                            listOf(
                                SegmentOption(stringResource(R.string.smaller)),
                                SegmentOption(stringResource(R.string.default_value)),
                                SegmentOption(stringResource(R.string.larger))
                            ),
                            when (preferences.textSize) {
                                -2 -> 0
                                2 -> 2
                                else -> 1
                            }
                        ) { index ->
                            model.preferences.update { it.copy(textSize = listOf(-2, 0, 2)[index]) }
                        }
                    }
                    SwitchRow(
                        stringResource(R.string.keep_screen_on),
                        preferences.keepScreenOn,
                        { on -> model.preferences.update { it.copy(keepScreenOn = on) } },
                        AppSymbol.SUN,
                        TileTint.amber,
                        subtitle = stringResource(R.string.while_terminal_open)
                    )
                }
            }
            item {
                val canLock = AppLock.canLock(activity)
                ListGroup(title = stringResource(R.string.privacy)) {
                    SwitchRow(
                        stringResource(R.string.app_lock),
                        preferences.lock,
                        { on -> scope.launch { model.lock.enable(activity, on) } },
                        AppSymbol.LOCK,
                        TileTint.green,
                        subtitle = stringResource(if (canLock) R.string.app_lock_message else R.string.set_device_lock),
                        enabled = canLock
                    )
                }
            }
            item {
                ListGroup(title = stringResource(R.string.documents)) {
                    val viewers = listOf(
                        Triple("docx", R.string.document_viewer, AppSymbol.FILE),
                        Triple("xlsx", R.string.sheet_viewer, AppSymbol.SHEET),
                        Triple("pptx", R.string.presentation_viewer, AppSymbol.PRESENTATION),
                        Triple("pdf", R.string.pdf_viewer, AppSymbol.FILE),
                        Triple("text", R.string.text_viewer, AppSymbol.COMPOSE)
                    )
                    viewers.forEachIndexed { index, (kind, label, symbol) ->
                        SwitchRow(
                            stringResource(label),
                            kind !in preferences.disabledViewers,
                            { on ->
                                model.preferences.update {
                                    it.copy(
                                        disabledViewers = if (on) {
                                            it.disabledViewers - kind
                                        } else {
                                            it.disabledViewers + kind
                                        }
                                    )
                                }
                            },
                            symbol,
                            listOf(
                                TileTint.blue,
                                TileTint.green,
                                TileTint.orange,
                                TileTint.red,
                                TileTint.graphite
                            )[index]
                        )
                    }
                    SwitchRow(stringResource(R.string.document_pages), preferences.documentPages, { on ->
                        model.preferences.update { it.copy(documentPages = on) }
                    }, AppSymbol.FILE, TileTint.teal)
                    SwitchRow(stringResource(R.string.document_dark), preferences.documentDark, { on ->
                        model.preferences.update { it.copy(documentDark = on) }
                    }, AppSymbol.APPEARANCE, TileTint.graphite)
                }
            }
            item {
                val language = AppLanguage.selected(context)
                val languages = listOf(
                    "" to stringResource(R.string.system_language), "en" to "English", "de" to "Deutsch",
                    "el" to "Ελληνικά",
                    "es" to "Español", "fr" to "Français", "it" to "Italiano", "ja" to "日本語", "ko" to "한국어",
                    "zh-Hans" to "简体中文"
                )
                ListGroup(title = stringResource(R.string.language)) {
                    MenuPicker(
                        stringResource(R.string.language),
                        languages.firstOrNull { it.first == language }?.second
                            ?: stringResource(R.string.system_language),
                        languages,
                        leading = { IconTile(AppSymbol.LANGUAGE, TileTint.purple) }
                    ) {
                        AppLanguage.choose(context, it)
                    }
                }
            }
            item {
                ListGroup(title = stringResource(R.string.notifications)) {
                    SwitchRow(
                        stringResource(R.string.notifications),
                        preferences.notifications,
                        { on ->
                            model.preferences.update { it.copy(notifications = on) }
                            model.updatePush()
                        },
                        AppSymbol.NOTIFICATIONS,
                        TileTint.red,
                        subtitle = notificationIssue ?: stringResource(R.string.notifications_description)
                    )
                    val permitted = context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) ==
                        android.content.pm.PackageManager.PERMISSION_GRANTED
                    ListButton({
                        if (context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) !=
                            android.content.pm.PackageManager.PERMISSION_GRANTED
                        ) {
                            model.preferences.notificationPermissionAsked()
                            permission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                        } else {
                            context.startActivity(
                                Intent(
                                    Settings.ACTION_APP_NOTIFICATION_SETTINGS
                                ).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                            )
                        }
                    }) {
                        ListLine(
                            stringResource(if (permitted) R.string.open_settings else R.string.enable_notifications),
                            leading = { IconTile(AppSymbol.SETTINGS, TileTint.graphite) }
                        ) { AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint) }
                    }
                }
            }
            if (Build.VERSION.SDK_INT >= 37) {
                item {
                    ListGroup {
                        ListLine(
                            stringResource(R.string.local_network),
                            subtitle = stringResource(R.string.local_network_message),
                            leading = { IconTile(AppSymbol.COMPUTER, TileTint.teal) }
                        ) {
                            KodosiTextButton(onClick = {
                                context.startActivity(
                                    Intent(
                                        Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                        android.net.Uri.fromParts("package", context.packageName, null)
                                    )
                                )
                            }) { Text(stringResource(R.string.open_settings)) }
                        }
                    }
                }
            }
            item {
                ListGroup(title = stringResource(R.string.about)) {
                    ListLine(
                        stringResource(R.string.version),
                        leading = { IconTile(AppSymbol.INFO, TileTint.blue) }
                    ) { Text(BuildConfig.VERSION_NAME, style = KodosiType.body, color = colors.inkMuted) }
                    ListButton(licenses) {
                        ListLine(stringResource(R.string.licenses), leading = {
                            IconTile(AppSymbol.FILE, TileTint.graphite)
                        }) {
                            AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint)
                        }
                    }
                    listOf(
                        Triple(R.string.privacy_policy, AppSymbol.LOCK, "https://kodosi.com/privacy"),
                        Triple(R.string.terms_of_use, AppSymbol.TEXT, "https://kodosi.com/terms"),
                        Triple(R.string.support, AppSymbol.CHAT, "https://kodosi.com/support")
                    ).forEachIndexed { index, (label, symbol, address) ->
                        ListButton({ links.open(address) }) {
                            ListLine(stringResource(label), leading = {
                                IconTile(symbol, listOf(TileTint.teal, TileTint.purple, TileTint.orange)[index])
                            }) {
                                AppIcon(AppSymbol.FORWARD, Modifier.size(10.dp), tint = colors.inkFaint)
                            }
                        }
                    }
                }
            }
            item {
                if (account.deleting) {
                    ListGroup {
                        ListLine(stringResource(R.string.confirm_account_deletion), leading = {
                            IconTile(AppSymbol.LOCK, TileTint.red)
                        }) {
                            KodosiTextButton(onClick = model.account::cancelDeletion) {
                                Text(stringResource(R.string.cancel))
                            }
                        }
                    }
                } else {
                    KodosiTextButton(onClick = { delete = true }) {
                        Text(stringResource(R.string.delete_account), color = KodosiTheme.colors.danger)
                    }
                }
            }
        }
    }
    if (signOut) {
        ConfirmDialog(
            stringResource(
                R.string.sign_out_title
            ),
            stringResource(R.string.sign_out_message),
            stringResource(R.string.sign_out),
            {
                signOut =
                    false
            },
            {
                signOut = false
                model.account.signOut()
            },
            destructive = true
        )
    }
    if (delete) {
        ConfirmDialog(
            stringResource(
                R.string.delete_account_title
            ),
            stringResource(R.string.delete_account_message),
            stringResource(R.string.delete_account),
            {
                delete =
                    false
            },
            {
                delete = false
                model.account.deleteAccount()
            },
            destructive = true
        )
    }
    if (approving) {
        TextEntryDialog(
            stringResource(R.string.add_device),
            stringResource(R.string.device_code),
            action = stringResource(R.string.check_device),
            extra = if (camera) {
                {
                    KodosiTextButton(onClick = { scanning = true }) {
                        AppIcon(AppSymbol.QR_CODE, tint = colors.accent)
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.scan_code))
                    }
                }
            } else {
                null
            },
            dismiss = { approving = false }
        ) { code ->
            val label = model.account.checkDevice(code) ?: return@TextEntryDialog
            checked = code to label
        }
    }
    if (scanning) {
        CodeScanDialog(
            CodeScanWords(
                title = stringResource(R.string.scan_code),
                hint = stringResource(R.string.point_at_device_code),
                other = stringResource(R.string.not_a_new_device_code),
                unavailable = stringResource(R.string.code_scanner_unavailable)
            ),
            accepts = DeviceCode::approve,
            found = { code ->
                scanning = false
                approving = false
                scope.launch {
                    model.runAction { model.account.checkDevice(code)?.let { label -> checked = code to label } }
                }
            },
            dismiss = { scanning = false }
        )
    }
    checked?.let { (code, label) ->
        ConfirmDialog(stringResource(R.string.approve_device_title), label, stringResource(R.string.approve), {
            checked =
                null
        }, {
            scope.launch {
                model.runAction {
                    model.account.approveDevice(code)
                    checked = null
                }
            }
        })
    }
    removing?.let { device ->
        ConfirmDialog(
            stringResource(
                R.string.remove
            ),
            if (device.recoveryKey) {
                stringResource(
                    R.string.replace_recovery_key_message
                )
            } else {
                stringResource(R.string.remove_device_message)
            },
            stringResource(R.string.remove),
            {
                removing =
                    null
            },
            {
                scope.launch {
                    model.runAction {
                        model.account.revokeDevice(device)
                        removing = null
                    }
                }
            },
            destructive = true
        )
    }
    if (creatingRecovery) {
        ConfirmDialog(
            stringResource(R.string.create_recovery_key),
            stringResource(
                if (account.devices.any {
                        it.recoveryKey
                    }
                ) {
                    R.string.replace_recovery_key_message
                } else {
                    R.string.recovery_message
                }
            ),
            stringResource(R.string.create),
            { creatingRecovery = false },
            {
                scope.launch {
                    model.runAction {
                        recovery = model.account.createRecoveryKey()
                        creatingRecovery = false
                    }
                }
            }
        )
    }
    recovery?.let { key ->
        val keyLabel = stringResource(R.string.recovery_key)
        KodosiDialog(
            onDismissRequest = {
                recovery = null
            },
            title = { Text(stringResource(R.string.recovery_key)) },
            properties = androidx.compose.ui.window.DialogProperties(
                securePolicy = androidx.compose.ui.window.SecureFlagPolicy.SecureOn
            ),
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(stringResource(R.string.keep_recovery_key))
                    SelectableWords(key)
                }
            },
            dismissButton = {
                KodosiTextButton(onClick = {
                    val clip = ClipData.newPlainText(keyLabel, key)
                    clip.description.extras =
                        PersistableBundle().apply { putBoolean("android.content.extra.IS_SENSITIVE", true) }
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
                }) { Text(stringResource(R.string.copy)) }
            },
            confirmButton = {
                KodosiTextButton(onClick = { recovery = null }) { Text(stringResource(R.string.close)) }
            }
        )
    }
}

@Composable
fun LicensesScreen(back: () -> Unit) {
    val context = LocalContext.current
    var selected by remember { mutableStateOf<String?>(null) }
    val files = remember { licenseFiles(context, "licenses") }
    Column(Modifier.fillMaxSize()) {
        PageHeader(stringResource(R.string.licenses), back)
        LazyColumn(Modifier.weight(1f).padding(horizontal = 16.dp)) {
            items(files, key = { it }) { file ->
                KodosiTextButton(onClick = {
                    selected = file
                }) { Text(file.removePrefix("licenses/").removeSuffix(".txt")) }
            }
        }
    }
    selected?.let { file ->
        KodosiDialog(
            onDismissRequest = { selected = null },
            title = { Text(file.substringAfterLast('/')) },
            text = {
                androidx.compose.foundation.text.selection.SelectionContainer {
                    Text(
                        remember(file) { context.assets.open(file).bufferedReader().use { it.readText() } },
                        modifier = Modifier.verticalScroll(rememberScrollState()),
                        style = KodosiType.footnote
                    )
                }
            },
            confirmButton = { KodosiTextButton(onClick = { selected = null }) { Text(stringResource(R.string.close)) } }
        )
    }
}

private fun licenseFiles(context: Context, path: String): List<String> =
    context.assets.list(path).orEmpty().flatMap { name ->
        val child = "$path/$name"
        if (context.assets.list(child).isNullOrEmpty()) listOf(child) else licenseFiles(context, child)
    }
