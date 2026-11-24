package com.kodosi.mobile.chat

import org.junit.Assert.assertEquals
import org.junit.Test

class ChatRowsTest {
    private var at = 0L

    @Test
    fun theAgentsWordsBetweenItsStepsShowAndTheStepsBetweenThemFold() {
        val items = listOf(
            person("p1", "Where is the tax?"),
            text("t1", "I will look."),
            tool("c1", ChatToolKind.READ, ChatToolState.DONE),
            text("t2", "It is in app.py."),
            ChatItem("e1", at++, ChatBody.Turn(TurnEnd.DONE, ms = 40_000))
        )

        val rows = ChatRow.rows(items, live = false)

        assertEquals(listOf("p1", "t1", "work.c1", "t2", "e1"), rows.map { it.id })
        assertEquals(ChatRow.Kind.Answer("I will look.", live = false), rows[1].kind)
        assertEquals(listOf("c1"), (rows[2].kind as ChatRow.Kind.Work).items.map { it.id })
        assertEquals(ChatRow.Kind.Answer("It is in app.py.", live = false), rows[3].kind)
        assertEquals(ChatRow.Kind.End(TurnEnd.DONE, 40_000), rows[4].kind)
    }

    @Test
    fun whileTheAgentWorksOnlyTheTextAfterTheLastStepAnswersTheTurn() {
        val working = listOf(
            person("p1", "Run the tests"),
            text("t1", "Running them now."),
            tool("c1", ChatToolKind.COMMAND, ChatToolState.RUNNING)
        )
        assertEquals(listOf("p1", "t1", "work.c1"), ChatRow.rows(working, live = true).map { it.id })

        val writing = working + ChatItem("t2", at++, ChatBody.Text("All 12 pass", live = true))
        val rows = ChatRow.rows(writing, live = true)

        assertEquals(listOf("p1", "t1", "work.c1", "t2"), rows.map { it.id })
        assertEquals(ChatRow.Kind.Answer("All 12 pass", live = true), rows.last().kind)
    }

    @Test
    fun aPlanThatWaitsForItsAnswerStaysWithTheWorkAndAPlanAtTheEndIsWhatTheTurnGives() {
        val asking = listOf(person("p1", "Plan the fix"), plan("s1", ChatToolState.ASKING))
        assertEquals(listOf("p1", "work.s1"), ChatRow.rows(asking, live = true).map { it.id })

        val done = listOf(person("p1", "Plan the fix"), plan("s1", ChatToolState.DONE))
        assertEquals(ChatRow.Kind.Plan("1. Fix total()", live = false), ChatRow.rows(done, live = false).last().kind)
    }

    @Test
    fun aShortTurnThatEndedWellLeavesNoEndAndATurnWithoutStepsShowsEachText() {
        val items = listOf(
            person("p1", "Hello"),
            text("t1", "Hi."),
            text("t2", "What should I do?"),
            ChatItem("e1", at++, ChatBody.Turn(TurnEnd.DONE, ms = 900)),
            person("p2", "Stop"),
            ChatItem("e2", at++, ChatBody.Turn(TurnEnd.STOPPED, ms = 900))
        )

        assertEquals(listOf("p1", "t1", "t2", "p2", "e2"), ChatRow.rows(items, live = false).map { it.id })
    }

    private fun person(id: String, text: String) = ChatItem(id, at++, ChatBody.Person(text))

    private fun text(id: String, text: String) = ChatItem(id, at++, ChatBody.Text(text))

    private fun tool(id: String, kind: ChatToolKind, state: ChatToolState) =
        ChatItem(id, at++, ChatBody.Tool(ChatTool("Tool", kind, state, "app.py")))

    private fun plan(id: String, state: ChatToolState) = ChatItem(
        id,
        at++,
        ChatBody.Tool(
            ChatTool("ExitPlanMode", ChatToolKind.PLAN, state, detail = ChatToolDetail.Plan("1. Fix total()"))
        )
    )
}
