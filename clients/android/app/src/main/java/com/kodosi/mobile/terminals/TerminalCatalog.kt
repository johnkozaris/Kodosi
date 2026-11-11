package com.kodosi.mobile.terminals

import java.text.Collator

/** The sign at the end of a terminal's row: what it waits for, or that it changed. */
enum class StatusForm { CHANGED, HAND, QUESTION, KEY, DONE, FAILED }

fun RuntimeSession.sign(unseen: Boolean = attention): StatusForm? = when (waitState) {
    ProgramStatus.State.BLOCKED -> when (programStatus?.kind) {
        ProgramStatus.Kind.QUESTION -> StatusForm.QUESTION
        ProgramStatus.Kind.AUTH -> StatusForm.KEY
        ProgramStatus.Kind.PERMISSION, null -> StatusForm.HAND
    }

    ProgramStatus.State.DONE -> if (unseen) StatusForm.DONE else null

    ProgramStatus.State.ERROR -> if (unseen) StatusForm.FAILED else null

    else -> if (unseen) StatusForm.CHANGED else null
}

data class TerminalGroup(val place: Place, val sessions: List<RuntimeSession>) {
    sealed interface Place {
        val id: String
        val name: String

        data class Computer(override val id: String, override val name: String) : Place

        data class Friend(override val id: String, override val name: String) : Place
    }

    /** Whether the computers of these terminals are offline. */
    val away: Boolean get() = sessions.isNotEmpty() && sessions.all { it.hostOffline }
}

object TerminalCatalog {
    /** Your terminals by computer, then the terminals of each friend. */
    fun groups(sessions: List<RuntimeSession>, unknownComputer: String, sharedWithYou: String): List<TerminalGroup> {
        val computers = LinkedHashMap<String, Pair<String, MutableList<RuntimeSession>>>()
        val friends = LinkedHashMap<String, Pair<String, MutableList<RuntimeSession>>>()
        for (session in sessions) {
            if (session.isOwner) {
                val name = session.hostName ?: session.ownerName ?: unknownComputer
                computers.getOrPut(session.hostDeviceId ?: name) { name to mutableListOf() }.second += session
            } else {
                val name = session.ownerName ?: sharedWithYou
                friends.getOrPut(session.ownerUserId ?: name) { name to mutableListOf() }.second += session
            }
        }
        return sorted(computers).map { (id, entry) ->
            TerminalGroup(TerminalGroup.Place.Computer(id, entry.first), entry.second)
        } +
            sorted(friends).map { (id, entry) ->
                TerminalGroup(TerminalGroup.Place.Friend(id, entry.first), entry.second)
            }
    }

    fun matching(sessions: List<RuntimeSession>, query: String): List<RuntimeSession> {
        val words = query.trim()
        if (words.isEmpty()) return sessions
        return sessions.filter { session ->
            listOfNotNull(
                session.name,
                session.activity,
                session.hostName,
                session.ownerName,
                session.projectName,
                session.folderName
            )
                .any { it.contains(words, ignoreCase = true) }
        }
    }

    private fun <T> sorted(groups: Map<String, Pair<String, T>>): List<Pair<String, Pair<String, T>>> {
        val collator = Collator.getInstance()
        return groups.entries.sortedWith { left, right ->
            collator.compare(left.value.first, right.value.first).takeIf { it != 0 } ?: left.key.compareTo(right.key)
        }.map { it.key to it.value }
    }
}
