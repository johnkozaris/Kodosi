package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.runtime.value
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
data class AgentAccount(val name: String, val command: String, val identity: String? = null, val folder: String? = null)

@Serializable
data class AgentInfo(
    val agent: String,
    val program: String,
    val installed: Boolean,
    val permissions: List<AgentPermissions>,
    val outputs: List<AgentOutput>,
    val defaultPermissions: AgentPermissions,
    val defaultOutput: AgentOutput,
    val accounts: List<AgentAccount> = emptyList(),
    val reportsUsage: Boolean = false
)

@Serializable
data class AgentModel(val id: String, val label: String, val efforts: List<String> = emptyList())

@Serializable
data class AgentPick(
    val agent: String,
    val account: String? = null,
    val model: String? = null,
    val effort: String? = null,
    val permissions: AgentPermissions? = null,
    val output: AgentOutput? = null
) {
    fun fields(): JsonObject = fields(
        "agent" to agent,
        "account" to account,
        "model" to model,
        "effort" to effort,
        "permissions" to permissions?.let { RuntimeJson.encodeToJsonElement(AgentPermissions.serializer(), it) },
        "output" to output?.let { RuntimeJson.encodeToJsonElement(AgentOutput.serializer(), it) }
    )
}

@Serializable
private data class AgentChoice(val agent: String?, val picks: List<AgentPick>)

@Serializable
data class FolderList(
    val path: String,
    val home: String,
    val parent: String? = null,
    val folders: List<String> = emptyList(),
    val recent: List<String> = emptyList()
) {
    fun child(name: String): String = path.trimEnd('/', '\\') + if (home.contains('\\')) "\\$name" else "/$name"
}

data class LauncherState(
    val loaded: Boolean = false,
    val agents: List<AgentInfo> = emptyList(),
    val agent: String? = null,
    val pick: AgentPick? = null,
    val models: List<AgentModel> = emptyList(),
    val modelsLoaded: Boolean = false,
    val efforts: List<String> = emptyList(),
    val folders: FolderList? = null,
    val usage: List<JsonObject> = emptyList(),
    val usageRead: Boolean = false,
    val readingUsage: Boolean = false
) {
    val loadingUsage: Boolean get() = readingUsage && !usageRead
}

class Launcher(private val commands: CommandSink, val computer: String) {
    private val mutableState = MutableStateFlow(LauncherState())
    val state: StateFlow<LauncherState> = mutableState
    private var picks: List<AgentPick> = emptyList()
    private var changes = 0

    suspend fun refresh() {
        val asOf = changes
        val result = commands.request("provider.agents", fields("computer" to computer)).value<JsonObject>("result")
        val agents = result.objects("agents").map {
            RuntimeJson.decodeFromJsonElement(AgentInfo.serializer(), it)
        }.filter { it.installed }
        mutableState.update { it.copy(loaded = true, agents = agents) }
        val choice = RuntimeJson.decodeFromJsonElement(AgentChoice.serializer(), result.getValue("choice"))
        if (changes == asOf) {
            mutableState.update { it.copy(agent = choice.agent?.takeIf { id -> agents.any { it.agent == id } }) }
            adopt(choice, asOf)
        }
        models(state.value.agent)
    }

    suspend fun models(agent: String?) {
        mutableState.update { it.copy(models = emptyList(), efforts = emptyList(), modelsLoaded = agent == null) }
        if (agent == null) return
        val asOf = changes
        val result = commands.request(
            "provider.agentModels",
            fields("computer" to computer, "agent" to agent)
        ).value<JsonObject>("result")
        if (state.value.agent != agent) return
        adopt(RuntimeJson.decodeFromJsonElement(AgentChoice.serializer(), result.getValue("choice")), asOf)
        mutableState.update {
            it.copy(
                modelsLoaded = true,
                models = result.objects("models").map { entry ->
                    RuntimeJson.decodeFromJsonElement(AgentModel.serializer(), entry)
                },
                efforts = result.objects("efforts").mapNotNull { option -> option.string("id") }
            )
        }
    }

    suspend fun chooseAgent(agent: String?) {
        val asOf = ++changes
        mutableState.update {
            it.copy(
                agent = agent,
                pick = picks.firstOrNull { pick -> pick.agent == agent },
                models = emptyList(),
                efforts = emptyList(),
                modelsLoaded = agent == null
            )
        }
        val choice = commands.request(
            "provider.choose",
            fields(
                "computer" to computer,
                "change" to fields("type" to "tab", "agent" to agent)
            )
        ).value<AgentChoice>("result")
        adopt(choice, asOf)
        if (asOf != changes) return
        models(agent)
    }

    suspend fun choose(pick: AgentPick) {
        val asOf = ++changes
        mutableState.update { it.copy(pick = pick) }
        picks = picks.filter { it.agent != pick.agent } + pick
        val choice = commands.request(
            "provider.choose",
            fields(
                "computer" to computer,
                "change" to JsonObject(pick.fields() + fields("type" to "pick"))
            )
        ).value<AgentChoice>("result")
        adopt(choice, asOf)
    }

    private fun adopt(choice: AgentChoice, asOf: Int) {
        if (asOf != changes) return
        picks = choice.picks
        mutableState.update { it.copy(pick = picks.firstOrNull { pick -> pick.agent == it.agent }) }
    }

    suspend fun folders(path: String? = null): FolderList {
        val fields = fields("computer" to computer)
        val reply = commands.request("folders.list", if (path == null) fields else fields + fields("path" to path))
        return reply.value<FolderList>("folders").also { folders -> mutableState.update { it.copy(folders = folders) } }
    }

    suspend fun usage() {
        mutableState.update { it.copy(readingUsage = true) }
        try {
            val result = commands.request("provider.usage", fields("computer" to computer)).value<JsonObject>("result")
            mutableState.update { it.copy(usage = result.objects("accounts"), usageRead = true) }
        } finally {
            mutableState.update { it.copy(readingUsage = false) }
        }
    }

    suspend fun start(name: String, directory: String?, prompt: String, project: String?): String? {
        val current = state.value
        val agent = current.agents.firstOrNull { it.agent == current.agent }
        val pick = current.pick
        val account = agent?.accounts?.firstOrNull { it.name == pick?.account } ?: agent?.accounts?.firstOrNull()
        val model = pick?.model?.takeIf { value -> current.models.any { it.id == value } }
        val allowed =
            current.models.firstOrNull { it.id == model }?.efforts?.takeIf { it.isNotEmpty() } ?: current.efforts
        val effort = pick?.effort?.takeIf(allowed::contains)
        val plan = agent?.let {
            AgentPick(
                it.agent,
                account?.name,
                model ?: "",
                effort ?: "",
                pick?.permissions?.takeIf(it.permissions::contains) ?: it.defaultPermissions,
                if (it.outputs.isEmpty()) null else pick?.output?.takeIf(it.outputs::contains) ?: it.defaultOutput
            ).fields()
        }
        val words = prompt.replace('\n', ' ').replace('\r', ' ').replace('\t', ' ').trim()
        return commands.request(
            "session.create",
            fields(
                "name" to name.trim(),
                "computer" to computer,
                "workingDir" to directory,
                "projectId" to project,
                "agent" to plan,
                "prompt" to words.takeIf { agent != null && it.isNotEmpty() }
            )
        ).string("sessionId")
    }
}
