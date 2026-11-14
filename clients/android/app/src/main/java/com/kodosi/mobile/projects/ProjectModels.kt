package com.kodosi.mobile.projects

import com.kodosi.mobile.people.Person
import kotlinx.serialization.Serializable

@Serializable
data class Project(val id: String, val name: String, val ownerUserId: String, val unread: Boolean = false)

@Serializable
data class ProjectInvitation(val id: String, val projectId: String, val projectName: String, val inviterName: String)

@Serializable
data class ProjectMember(
    val userId: String,
    val handle: String,
    val displayName: String,
    val isOwner: Boolean = false
) {
    val name: String get() = displayName.ifBlank { handle }
    val person: Person get() = Person(userId, handle, displayName)
}

@Serializable
data class ProjectMessage(
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
data class ProjectTask(
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
    val issue: ProjectIssue? = null
)

@Serializable
data class ProjectRepository(
    val id: String,
    val name: String,
    val url: String,
    val host: String,
    val owner: String,
    val repository: String
)

@Serializable
data class ProjectIssue(
    val repositoryId: String,
    val number: Long,
    val title: String,
    val url: String,
    val closed: Boolean = false,
    val assignees: List<String> = emptyList()
)

@Serializable
data class ProjectSnapshot(
    val projectId: String,
    val messages: List<ProjectMessage> = emptyList(),
    val tasks: List<ProjectTask> = emptyList(),
    val repositories: List<ProjectRepository> = emptyList(),
    val hasOlder: Boolean = false,
    val historyTrimmed: Boolean = false
) {
    fun merging(previous: ProjectSnapshot): ProjectSnapshot = copy(
        messages = (previous.messages + messages).associateBy { it.id }.values.sortedBy { it.sequence },
        tasks = (previous.tasks + tasks).groupBy { it.id }.values.map { versions -> versions.maxBy { it.version } }
            .sortedBy { it.id },
        hasOlder = previous.hasOlder && hasOlder,
        historyTrimmed = previous.historyTrimmed || historyTrimmed
    )
}

data class ProjectDetail(val project: Project, val members: List<ProjectMember>)

data class ProjectsState(
    val projects: List<Project> = emptyList(),
    val invitations: List<ProjectInvitation> = emptyList(),
    val loaded: Boolean = false,
    val truncated: Boolean = false,
    val snapshots: Map<String, ProjectSnapshot> = emptyMap(),
    val details: Map<String, ProjectDetail> = emptyMap(),
    val unread: Set<String> = emptySet()
) {
    val badge: Int get() = invitations.size + (projects.filter { it.unread }.map { it.id }.toSet() + unread).size
}
