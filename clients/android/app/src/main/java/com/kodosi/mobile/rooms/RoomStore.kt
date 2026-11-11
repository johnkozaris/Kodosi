package com.kodosi.mobile.rooms

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.clippedUtf8
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.value
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.JsonObject

class RoomStore(private val commands: CommandSink) {
    private val mutableState = MutableStateFlow(RoomsState())
    val state: StateFlow<RoomsState> = mutableState
    val drafts = mutableMapOf<String, String>()
    private val seen = mutableMapOf<String, Long>()
    private var userId: String? = null
    private var generation = 0
    var reading: String? = null

    fun reset(userId: String?) {
        generation++
        this.userId = userId
        mutableState.value = RoomsState()
        drafts.clear()
        seen.clear()
        reading = null
    }

    fun room(id: String): Room? = state.value.details[id]?.room ?: state.value.rooms.firstOrNull { it.id == id }

    fun receive(event: RuntimeEvent) {
        when (event.type) {
            "missions.snapshot" -> {
                val rooms = event.value<List<Room>>("missions")
                val truncated = event.bool("missionsTruncated") == true
                val present = rooms.map { it.id }.toSet()
                mutableState.update {
                    it.copy(
                        rooms = rooms,
                        invitations = event.value("invitations"),
                        loaded = true,
                        truncated = truncated || event.bool("invitationsTruncated") == true,
                        snapshots = if (truncated) it.snapshots else it.snapshots.filterKeys(present::contains),
                        details = if (truncated) it.details else it.details.filterKeys(present::contains),
                        unread = it.unread.intersect(present)
                    )
                }
            }

            "room.snapshot", "room.result" -> if (event["room"] is JsonObject) apply(event.value("room"))
        }
    }

    private fun apply(next: RoomSnapshot) {
        mutableState.update { current ->
            val previous = current.snapshots[next.roomId]
            val fresh = previous != null && next.messages.any {
                it.sequence > (previous.messages.maxOfOrNull { message -> message.sequence } ?: 0) && !it.isOwn(userId)
            }
            current.copy(
                snapshots = current.snapshots + (next.roomId to (previous?.let(next::merging) ?: next)),
                unread = if (fresh && reading != next.roomId) current.unread + next.roomId else current.unread
            )
        }
    }

    fun refresh() = commands.perform("mission.list")

    suspend fun open(id: String) {
        val epoch = generation
        val reply = commands.request("mission.open", fields("missionId" to id))
        if (generation != epoch) return
        mutableState.update {
            it.copy(
                details =
                    it.details + (id to RoomDetail(reply.value("mission"), reply.value("members")))
            )
        }
    }

    suspend fun read(id: String, before: Long? = null) {
        act(id, if (before == null) fields("type" to "read") else fields("type" to "read", "before" to before))
    }

    fun markRead(id: String) {
        val sequence = state.value.snapshots[id]?.messages?.maxOfOrNull { it.sequence } ?: return
        if (sequence > (seen[id] ?: 0)) {
            seen[id] = sequence
            commands.perform("mission.seen", fields("missionId" to id, "sequence" to sequence))
        }
        mutableState.update {
            it.copy(
                unread = it.unread - id,
                rooms = it.rooms.map { room ->
                    if (room.id ==
                        id
                    ) {
                        room.copy(unread = false)
                    } else {
                        room
                    }
                }
            )
        }
    }

    suspend fun act(id: String, action: JsonObject): RuntimeEvent =
        commands.request("room.command", fields("roomId" to id, "action" to action))

    suspend fun post(id: String, text: String) {
        val message = text.trim()
        if (message.isEmpty()) return
        act(id, fields("type" to "post", "text" to message))
        if (drafts[id] == text) drafts.remove(id)
    }

    suspend fun create(name: String): String {
        val id = UuidV7.generate()
        commands.request("mission.create", fields("requestId" to id, "name" to name.trim()))
        return id
    }

    suspend fun respond(invitation: RoomInvitation, accept: Boolean) {
        commands.request(
            if (accept) "mission.invitation.accept" else "mission.invitation.reject",
            fields("invitationId" to invitation.id)
        )
    }

    suspend fun createTask(
        id: String,
        title: String,
        description: String,
        repositories: Set<String>,
        issueRepository: String? = null
    ) {
        val action = fields(
            "type" to "createTask",
            "title" to title.trim().clippedUtf8(256),
            "description" to description.trim().clippedUtf8(16384),
            "repositoryIds" to repositories.sorted()
        )
        act(
            id,
            if (issueRepository ==
                null
            ) {
                action
            } else {
                JsonObject(action + fields("issueRepositoryId" to issueRepository))
            }
        )
    }

    suspend fun updateTask(id: String, task: RoomTask, change: String, note: String? = null) {
        val action = fields("type" to "updateTask", "taskId" to task.id, "change" to change)
        act(id, if (note.isNullOrBlank()) action else JsonObject(action + fields("note" to note.trim())))
    }

    suspend fun addRepository(id: String, address: String) {
        act(id, fields("type" to "addRepository", "url" to address.trim()))
    }

    suspend fun issues(id: String, repository: String): List<RoomIssue> =
        act(id, fields("type" to "issues", "repositoryId" to repository)).value("issues")

    suspend fun importIssue(id: String, issue: RoomIssue) {
        act(id, fields("type" to "importIssue", "repositoryId" to issue.repositoryId, "number" to issue.number))
    }
}
