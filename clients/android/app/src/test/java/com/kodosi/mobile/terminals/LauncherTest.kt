package com.kodosi.mobile.terminals

import com.kodosi.mobile.runtime.SentCommands
import com.kodosi.mobile.runtime.string
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class LauncherTest {
    private val sent = SentCommands()
    private val launcher = Launcher(sent.sink, "computer")

    @Test
    fun aFolderOfTheHomeShowsFromTheHome() {
        val folders = FolderList(path = "/home/maya/shop", home = "/home/maya")
        assertEquals("~", folders.shown("/home/maya"))
        assertEquals("~/shop/src", folders.shown("/home/maya/shop/src"))
        assertEquals("/home/mayas-notes", folders.shown("/home/mayas-notes"))
        assertEquals("/srv/www", folders.shown("/srv/www"))
        val windows = FolderList(path = "C:\\Users\\maya", home = "C:\\Users\\maya")
        assertEquals("~\\shop", windows.shown("C:\\Users\\maya\\shop"))
    }

    @Test
    fun startsTheShownAccountAndDropsAnEffortTheModelDoesNotOffer() = runTest {
        val loading = async(start = CoroutineStart.UNDISPATCHED) { launcher.refresh() }
        sent.reply(
            "provider.agents",
            """"result":{"agents":[{"agent":"claude","program":"claude","installed":true,
                "accounts":[{"name":"work","command":"claude"}],"permissions":["ask","unrestricted"],
                "outputs":["quiet","standard","detailed"],"defaultPermissions":"unrestricted","defaultOutput":"quiet"}],
                "choice":{"agent":"claude","picks":[{"agent":"claude","account":"work","model":"opus","effort":"max"}]}}"""
        )
        runCurrent()
        sent.reply(
            "provider.agentModels",
            """"result":{"models":[{"id":"opus","label":"Opus","efforts":["high"]}],"efforts":[{"id":"max"}],
                "choice":{"agent":"claude","picks":[{"agent":"claude","account":"work","model":"opus","effort":"max"}]}}"""
        )
        loading.await()
        val starting = async(start = CoroutineStart.UNDISPATCHED) {
            launcher.start(" Work ", "/code", " a\nb\tc ", "project")
        }
        val command = sent.last("session.create")
        assertEquals("work", command["agent"]!!.jsonObject.string("account"))
        assertEquals("opus", command["agent"]!!.jsonObject.string("model"))
        assertEquals("", command["agent"]!!.jsonObject.string("effort"))
        assertEquals("a b c", command.string("prompt"))
        assertEquals("project", command.string("projectId"))
        sent.reply("session.create", """"sessionId":"terminal"""")
        assertEquals("terminal", starting.await())
    }

    @Test
    fun startsTheRuntimeResolvedDefaultModelAndModes() = runTest {
        val loading = async(start = CoroutineStart.UNDISPATCHED) { launcher.refresh() }
        sent.reply(
            "provider.agents",
            """"result":{"agents":[{"agent":"claude","program":"claude","installed":true,
                "accounts":[{"name":"work","command":"claude"}],"permissions":["ask","auto","unrestricted"],
                "outputs":["quiet","standard","detailed"],"defaultPermissions":"unrestricted","defaultOutput":"quiet",
                "fast":true}],"choice":{"agent":"claude","picks":[]}}"""
        )
        runCurrent()
        sent.reply(
            "provider.agentModels",
            """"result":{"models":[{"id":"sonnet","label":"Sonnet","efforts":["high"]}],"efforts":[{"id":"high"}],
                "choice":{"agent":"claude","picks":[{"agent":"claude","account":"work","model":"sonnet",
                "effort":"high","permissions":"unrestricted","output":"quiet","fast":true}]}}"""
        )
        loading.await()
        val starting = async(start = CoroutineStart.UNDISPATCHED) { launcher.start("Work", null, "", null) }
        val plan = sent.last("session.create")["agent"]!!.jsonObject
        assertEquals("sonnet", plan.string("model"))
        assertEquals("high", plan.string("effort"))
        assertEquals("unrestricted", plan.string("permissions"))
        assertEquals("quiet", plan.string("output"))
        assertEquals(JsonPrimitive(true), plan["fast"])
        sent.reply("session.create", """"sessionId":"terminal"""")
        starting.await()
    }

    @Test
    fun changingAnAccountPreservesTheRuntimePermissionOutputAndFastChoice() = runTest {
        val loading = async(start = CoroutineStart.UNDISPATCHED) { launcher.refresh() }
        val choice = """"choice":{"agent":"claude","picks":[{"agent":"claude","account":"work",
            "permissions":"auto","output":"detailed","fast":true}]}"""
        sent.reply(
            "provider.agents",
            """"result":{"agents":[{"agent":"claude","program":"claude","installed":true,
                "accounts":[{"name":"work","command":"claude"},{"name":"personal","command":"claude-personal"}],
                "permissions":["ask","auto","unrestricted"],"outputs":["quiet","standard","detailed"],
                "defaultPermissions":"unrestricted","defaultOutput":"quiet","fast":true}],$choice}"""
        )
        runCurrent()
        sent.reply("provider.agentModels", """"result":{"models":[],"efforts":[],$choice}""")
        loading.await()
        val choosing = async(start = CoroutineStart.UNDISPATCHED) {
            launcher.choose(launcher.state.value.pick!!.copy(account = "personal"))
        }
        val change = sent.last("provider.choose")["change"]!!.jsonObject
        assertEquals("personal", change.string("account"))
        assertEquals("auto", change.string("permissions"))
        assertEquals("detailed", change.string("output"))
        assertEquals(JsonPrimitive(true), change["fast"])
        sent.reply(
            "provider.choose",
            """"result":{"agent":"claude","picks":[{"agent":"claude","account":"personal",
                "permissions":"auto","output":"detailed","fast":true}]}"""
        )
        choosing.await()
    }

    @Test
    fun keepsANewerPickWhenModelDiscoveryAnswersLate() = runTest {
        val loading = async(start = CoroutineStart.UNDISPATCHED) { launcher.refresh() }
        sent.reply(
            "provider.agents",
            """"result":{"agents":[{"agent":"claude","program":"claude","installed":true,
                "accounts":[{"name":"work","command":"claude"},{"name":"personal","command":"claude-personal"}],
                "permissions":["ask","unrestricted"],"outputs":[],"defaultPermissions":"unrestricted",
                "defaultOutput":"quiet"}],"choice":{"agent":"claude","picks":[{"agent":"claude","account":"work"}]}}"""
        )
        runCurrent()
        val choosing = async(start = CoroutineStart.UNDISPATCHED) {
            launcher.choose(launcher.state.value.pick!!.copy(account = "personal"))
        }
        sent.reply(
            "provider.choose",
            """"result":{"agent":"claude","picks":[{"agent":"claude","account":"personal"}]}"""
        )
        choosing.await()
        sent.reply(
            "provider.agentModels",
            """"result":{"models":[{"id":"sonnet","label":"Sonnet"}],"efforts":[],
                "choice":{"agent":"claude","picks":[{"agent":"claude","account":"work"}]}}"""
        )
        loading.await()
        assertEquals("personal", launcher.state.value.pick?.account)
        assertEquals("sonnet", launcher.state.value.models.single().id)
    }

    @Test
    fun aShellStartDoesNotLeakAnAgentOrPrompt() = runTest {
        val starting =
            async(start = CoroutineStart.UNDISPATCHED) { launcher.start("", null, "not a shell command", null) }
        val command = sent.last("session.create")
        assertEquals(JsonNull, command["agent"])
        assertEquals(JsonNull, command["prompt"])
        sent.reply("session.create", """"sessionId":"shell"""")
        assertEquals("shell", starting.await())
    }
}
