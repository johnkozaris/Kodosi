package com.kodosi.mobile.rooms

import kotlinx.serialization.Serializable

@Serializable
data class Room(val id: String, val name: String, val ownerUserId: String, val unread: Boolean = false)

@Serializable
data class RoomInvitation(val id: String, val missionId: String, val missionName: String, val inviterName: String)

@Serializable
data class RoomMember(val userId: String, val handle: String, val displayName: String, val isOwner: Boolean = false) {
    val name: String get() = displayName.ifBlank { handle }
}

@Serializable
data class RoomMessage(
    val id: String,
    val sequence: Long,
    val authorId: String,
    val authorName: String,
    val agent: String? = null,
    val terminalId: String? = null,
    val text: String,
    val createdAt: String
) {
    fun isOwn(userId: String?): Boolean = agent == null && authorId == userId
}

@Serializable
data class RoomTask(
    val id: String,
    val version: Long,
    val title: String,
    val description: String,
    val closed: Boolean = false,
    val assignedTo: String? = null,
    val assignedName: String? = null,
    val terminalId: String? = null,
    val repositoryIds: List<String> = emptyList(),
    val note: String? = null,
    val issue: RoomIssue? = null
)

@Serializable
data class RoomRepository(
    val id: String,
    val name: String,
    val url: String,
    val host: String,
    val owner: String,
    val repository: String
)

@Serializable
data class RoomIssue(
    val repositoryId: String,
    val number: Long,
    val title: String,
    val url: String,
    val closed: Boolean = false,
    val assignees: List<String> = emptyList()
)

@Serializable
data class RoomSnapshot(
    val roomId: String,
    val messages: List<RoomMessage> = emptyList(),
    val tasks: List<RoomTask> = emptyList(),
    val repositories: List<RoomRepository> = emptyList(),
    val hasOlder: Boolean = false,
    val historyTrimmed: Boolean = false
) {
    fun merging(previous: RoomSnapshot): RoomSnapshot = copy(
        messages = (previous.messages + messages).associateBy { it.id }.values.sortedBy { it.sequence },
        tasks = (previous.tasks + tasks).groupBy { it.id }.values.map { versions -> versions.maxBy { it.version } }
            .sortedBy { it.id },
        hasOlder = previous.hasOlder && hasOlder,
        historyTrimmed = previous.historyTrimmed || historyTrimmed
    )
}

data class RoomDetail(val room: Room, val members: List<RoomMember>)

data class RoomsState(
    val rooms: List<Room> = emptyList(),
    val invitations: List<RoomInvitation> = emptyList(),
    val loaded: Boolean = false,
    val truncated: Boolean = false,
    val snapshots: Map<String, RoomSnapshot> = emptyMap(),
    val details: Map<String, RoomDetail> = emptyMap(),
    val unread: Set<String> = emptySet()
) {
    val badge: Int get() = invitations.size + (rooms.filter { it.unread }.map { it.id }.toSet() + unread).size
}
