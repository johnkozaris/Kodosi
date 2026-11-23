package com.kodosi.mobile.chat

import com.kodosi.mobile.terminals.AgentModel
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatWordsTest {
    @Test
    fun theSummaryCountsEachKindInTheOrderItCameAndAFileOnce() {
        val tools = listOf(
            tool(ChatToolKind.COMMAND, "npm test"),
            tool(ChatToolKind.READ, "app.py"),
            tool(ChatToolKind.COMMAND, "npm test"),
            tool(ChatToolKind.READ, "app.py"),
            tool(ChatToolKind.EDIT, "app.py"),
            tool(ChatToolKind.READ, "notes.md")
        )

        assertEquals(
            listOf(ChatToolKind.COMMAND to 2, ChatToolKind.READ to 2, ChatToolKind.EDIT to 1),
            ChatWords.counts(tools)
        )
        assertEquals(ChatToolKind.READ, ChatWords.main(tools))
    }

    @Test
    fun aModelThatTheAgentNamesByItsFullIdTakesTheLaunchersNameAndItsVersion() {
        val models =
            listOf(AgentModel("sonnet", "Sonnet"), AgentModel("opus", "Opus"), AgentModel("gpt-6-luna", "GPT-6-Luna"))

        assertEquals("Sonnet 5.5", ChatWords.modelName("claude-sonnet-5-5", models))
        assertEquals("Sonnet 4.5", ChatWords.modelName("claude-sonnet-4-5-20250929", models))
        assertEquals("Opus 5.5", ChatWords.modelName("claude-opus-5-5[1m]", models))
        assertEquals("GPT-6-Luna", ChatWords.modelName("gpt-6-luna", models))
        assertEquals("mai-code-2", ChatWords.modelName("mai-code-2", models))
    }

    @Test
    fun aFenceThatIsStillOpenWhileTheAgentWritesHoldsTheRest() {
        val blocks = ProseBlock.blocks("## Fix\n- one\n| a | b |\nThen:\n```\nnpm test\nmore")

        assertEquals(
            listOf(
                ProseBlock.Heading("Fix"),
                ProseBlock.Lines("• one"),
                ProseBlock.Table(ProseTable(emptyList(), listOf(listOf("a", "b")), emptyList())),
                ProseBlock.Lines("Then:"),
                ProseBlock.Code("npm test\nmore")
            ),
            blocks
        )
    }

    @Test
    fun aTableHasItsColumnsTheirSidesAndTheBarsThatItsWordsHold() {
        val table = ProseTable.of(
            listOf("| SKU | price \\| EUR | note |", "|:---|---:|:-:|", "| MUG-01 | 2400 | `a` |", "| BOWL-02 | 3800 |")
        )

        assertEquals(listOf("SKU", "price | EUR", "note"), table.header)
        assertEquals(listOf(listOf("MUG-01", "2400", "`a`"), listOf("BOWL-02", "3800")), table.rows)
        assertEquals(
            listOf(ProseTable.Side.START, ProseTable.Side.END, ProseTable.Side.CENTER, ProseTable.Side.START),
            (0..3).map(table::side)
        )
        assertEquals(3, table.columns)
    }

    private fun tool(kind: ChatToolKind, target: String) = ChatTool("Tool", kind, ChatToolState.DONE, target)
}
