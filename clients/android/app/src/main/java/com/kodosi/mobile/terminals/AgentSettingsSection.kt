package com.kodosi.mobile.terminals

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.runtime.value
import com.kodosi.mobile.ui.AgentMark
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.MenuPicker
import com.kodosi.mobile.ui.ProgressCaption
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

@Composable
internal fun AgentSettingsSection(model: AppModel, session: RuntimeSession) {
    val live = session.programLive ?: return
    val readsModels = session.isOwner && session.hostDeviceId != null && (live.model || live.effort)
    if (!readsModels && live.permissions.isEmpty()) return
    val settings = session.programSettings ?: AgentSettings()
    val pending = session.programPending ?: AgentSettings()
    val currentModel = pending.model ?: settings.model
    val currentEffort = if (pending.model != null) pending.effort else pending.effort ?: settings.effort
    val currentPermissions = pending.permissions ?: settings.permissions
    val scope = rememberCoroutineScope()
    var models by remember(session.id, session.program) { mutableStateOf<List<AgentModel>?>(null) }
    var efforts by remember(session.id, session.program) { mutableStateOf(emptyList<String>()) }
    var failure by remember(session.id, session.program) { mutableStateOf<String?>(null) }
    var attempt by remember { mutableIntStateOf(0) }
    var changing by remember { mutableStateOf(false) }
    LaunchedEffect(session.id, session.program, readsModels, attempt) {
        if (!readsModels) return@LaunchedEffect
        failure = null
        try {
            val result = model.commands.request(
                "provider.agentModels",
                fields("computer" to session.hostDeviceId, "agent" to session.agent.name.lowercase())
            ).value<JsonObject>("result")
            models = result.objects("models").map { RuntimeJson.decodeFromJsonElement(AgentModel.serializer(), it) }
            efforts = result.objects("efforts").mapNotNull { it.string("id") }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            failure = error.localizedMessage ?: model.words.didNotWork
        }
    }
    val change: (AgentSettings) -> Unit = { choice ->
        changing = true
        scope.launch {
            try {
                model.terminals.act(
                    "session.agent",
                    session,
                    fields("change" to RuntimeJson.encodeToJsonElement(AgentSettings.serializer(), choice))
                )
                failure = null
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                failure = error.localizedMessage ?: model.words.didNotWork
            } finally {
                changing = false
            }
        }
    }

    @Composable
    fun shown(value: String, waits: Boolean): String =
        if (waits) stringResource(R.string.agent_after_turn, value) else value

    ListLine(agentLabel(session.agent), leading = { AgentMark(session.agent, 24.dp, account = session.programAccount) })
    if (readsModels && models == null && failure == null) {
        ProgressCaption(stringResource(R.string.loading))
    }
    if (live.model && !models.isNullOrEmpty()) {
        MenuPicker(
            stringResource(R.string.model),
            shown(
                models?.firstOrNull { it.id == currentModel }?.label ?: currentModel
                    ?: stringResource(R.string.default_value),
                pending.model != null
            ),
            models.orEmpty().map { it.id to it.label },
            leading = { AppIcon(AppSymbol.DOSI) },
            enabled = !changing && session.canControl
        ) { if (it != currentModel) change(AgentSettings(model = it)) }
    }
    val levels = models?.firstOrNull { it.id == currentModel }?.efforts?.takeIf { it.isNotEmpty() } ?: efforts
    if (live.effort && levels.isNotEmpty()) {
        MenuPicker(
            stringResource(R.string.effort),
            shown(
                currentEffort?.let { effortLabel(it) } ?: stringResource(R.string.default_value),
                pending.effort != null
            ),
            levels.map { it to effortLabel(it) },
            leading = { AppIcon(AppSymbol.TEXT_SIZE) },
            enabled = !changing && session.canControl
        ) { if (it != currentEffort) change(AgentSettings(effort = it)) }
    }
    if (live.permissions.isNotEmpty()) {
        val options = AgentPermissions.entries.filter { it in live.permissions || it == currentPermissions }
        MenuPicker(
            stringResource(R.string.agent_permissions),
            shown(currentPermissions?.label() ?: stringResource(R.string.default_value), pending.permissions != null),
            options.map { it.name to it.label() },
            leading = { AppIcon(AppSymbol.LOCK) },
            enabled = !changing && session.canControl,
            details = options.associate { it.name to it.detail(session.agent) }
        ) {
            val permissions = AgentPermissions.valueOf(it)
            if (permissions != currentPermissions) change(AgentSettings(permissions = permissions))
        }
    }
    if (live.keeps && readsModels && models != null) {
        Text(
            stringResource(R.string.agent_keeps_settings, agentLabel(session.agent)),
            Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            style = KodosiType.footnote,
            color = KodosiTheme.colors.inkMuted
        )
    }
    failure?.let {
        Column(Modifier.padding(horizontal = 16.dp)) {
            ErrorNote(it)
            if (readsModels && models == null) {
                KodosiTextButton(onClick = { attempt++ }) { Text(stringResource(R.string.try_again)) }
            }
        }
    }
}

@Composable
internal fun AgentPermissions.label(): String = stringResource(
    when (this) {
        AgentPermissions.ASK -> R.string.agent_ask
        AgentPermissions.AUTO -> R.string.agent_auto
        AgentPermissions.UNRESTRICTED -> R.string.agent_unrestricted
    }
)

@Composable
internal fun AgentPermissions.detail(agent: AgentKind): String = stringResource(
    when (this) {
        AgentPermissions.ASK -> R.string.agent_ask_detail

        AgentPermissions.UNRESTRICTED -> R.string.agent_unrestricted_detail

        AgentPermissions.AUTO -> when (agent) {
            AgentKind.CLAUDE, AgentKind.COPILOT -> R.string.agent_auto_check_detail
            AgentKind.CODEX -> R.string.agent_auto_folder_detail
            AgentKind.GEMINI -> R.string.agent_auto_edit_detail
            else -> R.string.agent_auto_detail
        }
    }
)

@Composable
internal fun AgentOutput.label(): String = stringResource(
    when (this) {
        AgentOutput.QUIET -> R.string.agent_quiet
        AgentOutput.STANDARD -> R.string.agent_standard
        AgentOutput.DETAILED -> R.string.agent_detailed
    }
)

@Composable
internal fun AgentOutput.detail(): String = stringResource(
    when (this) {
        AgentOutput.QUIET -> R.string.agent_quiet_detail
        AgentOutput.STANDARD -> R.string.agent_standard_detail
        AgentOutput.DETAILED -> R.string.agent_detailed_detail
    }
)

@Composable
internal fun effortLabel(id: String): String = when (id) {
    "none" -> stringResource(R.string.agent_effort_none)
    "minimal" -> stringResource(R.string.agent_effort_minimal)
    "low" -> stringResource(R.string.agent_effort_low)
    "medium" -> stringResource(R.string.agent_effort_medium)
    "high" -> stringResource(R.string.agent_effort_high)
    "xhigh" -> stringResource(R.string.agent_effort_xhigh)
    "max" -> stringResource(R.string.agent_effort_max)
    else -> id
}
