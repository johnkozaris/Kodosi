package com.kodosi.mobile.chat

import org.junit.Assert.assertEquals
import org.junit.Test

class ChatDockTest {
    @Test
    fun onlyTheHelpersThatRunShowInTheOrderThatTheAgentStartedThem() {
        val reading = ChatStep(ChatToolKind.READ, "src/tax.ts")
        val explore = ChatToolDetail.Agent("Explore", 4, reading, "Found the rate")
        val items = listOf(
            ChatItem("p1", 0, ChatBody.Person("Fix the tax")),
            helper("h1", ChatToolState.RUNNING, "Find the tax code", explore),
            helper("h2", ChatToolState.DONE, "Read the docs", explore),
            helper("h3", ChatToolState.PREPARING, "Check the cart"),
            ChatItem("c1", 0, ChatBody.Tool(ChatTool("Bash", ChatToolKind.COMMAND, ChatToolState.RUNNING, "npm test"))),
            helper("h4", ChatToolState.RUNNING, "Check the tests")
        )

        assertEquals(
            listOf(
                ChatHelper("h1", "Explore", "Find the tax code", 4, reading, "Found the rate"),
                ChatHelper("h4", "", "Check the tests", 0, null, "")
            ),
            items.workingHelpers
        )
    }

    private fun helper(id: String, state: ChatToolState, task: String, detail: ChatToolDetail? = null) =
        ChatItem(id, 0, ChatBody.Tool(ChatTool("Task", ChatToolKind.AGENT, state, task, detail)))
}
