package com.kodosi.mobile.chat

/**
 * The chat as a person reads it: text the agent writes during a turn appears as its own message,
 * and each run of steps between those messages folds into a single row.
 */
data class ChatRow(val id: String, val at: Long, val kind: Kind) {
    sealed interface Kind {
        data class Person(val text: String, val by: ChatPerson?) : Kind

        data class Answer(val text: String, val live: Boolean) : Kind

        data class Plan(val text: String, val live: Boolean) : Kind

        /** A turn's calls between two of the agent's messages, as far as they have come. */
        data class Work(val items: List<ChatItem>) : Kind

        data class End(val end: TurnEnd, val ms: Long) : Kind

        data class Note(val note: ChatNote) : Kind
    }

    companion object {
        const val LONG_TURN_MS = 30_000L

        fun rows(items: List<ChatItem>, live: Boolean): List<ChatRow> = buildList {
            val turn = mutableListOf<ChatItem>()
            fun fold(ended: Boolean) {
                if (turn.isEmpty()) return
                val answer = when {
                    ended -> turn.indexOfLast { it.answers }.takeIf { it >= 0 }
                    turn.last().answers -> turn.lastIndex
                    else -> null
                }
                val work = mutableListOf<ChatItem>()
                fun endWork() {
                    val first = work.firstOrNull() ?: return
                    add(ChatRow("work.${first.id}", first.at, Kind.Work(work.toList())))
                    work.clear()
                }
                turn.forEachIndexed { index, item ->
                    val body = item.body
                    when {
                        index == answer -> {
                            endWork()
                            answerRow(item, ended)?.let(::add)
                        }

                        body is ChatBody.Text -> {
                            endWork()
                            add(ChatRow(item.id, item.at, Kind.Answer(body.text, false)))
                        }

                        else -> work += item
                    }
                }
                endWork()
                turn.clear()
            }
            for (item in items) {
                when (val body = item.body) {
                    is ChatBody.Person -> {
                        fold(ended = true)
                        add(ChatRow(item.id, item.at, Kind.Person(body.text, body.by)))
                    }

                    is ChatBody.Text, is ChatBody.Tool -> turn += item

                    is ChatBody.Turn -> {
                        fold(ended = true)
                        if (body.end != TurnEnd.DONE || body.ms >= LONG_TURN_MS) {
                            add(ChatRow(item.id, item.at, Kind.End(body.end, body.ms)))
                        }
                    }

                    is ChatBody.Note -> {
                        fold(ended = true)
                        add(ChatRow(item.id, item.at, Kind.Note(body.note)))
                    }
                }
            }
            fold(ended = !live)
        }

        private fun answerRow(item: ChatItem, ended: Boolean): ChatRow? = when (val body = item.body) {
            is ChatBody.Text -> ChatRow(item.id, item.at, Kind.Answer(body.text, body.live && !ended))

            is ChatBody.Tool -> {
                val plan = (body.tool.detail as? ChatToolDetail.Plan)?.text.orEmpty()
                if (plan.isEmpty()) {
                    null
                } else {
                    ChatRow(item.id, item.at, Kind.Plan(plan, body.tool.state.isOpen && !ended))
                }
            }

            else -> null
        }
    }
}

// A plan awaiting approval is shown in ChatAskCard, so only treat it as the turn's answer once
// it is no longer asking.
private val ChatItem.answers: Boolean
    get() = when (val body = body) {
        is ChatBody.Text -> true
        is ChatBody.Tool -> body.tool.detail is ChatToolDetail.Plan && body.tool.state != ChatToolState.ASKING
        else -> false
    }

val List<ChatItem>.tools: List<ChatTool> get() = mapNotNull { (it.body as? ChatBody.Tool)?.tool }
