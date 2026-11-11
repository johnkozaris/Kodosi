package com.kodosi.mobile.terminals

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class AgentPermissions {
    @SerialName("ask")
    ASK,

    @SerialName("auto")
    AUTO,

    @SerialName("unrestricted")
    UNRESTRICTED
}

@Serializable
enum class AgentOutput {
    @SerialName("quiet")
    QUIET,

    @SerialName("standard")
    STANDARD,

    @SerialName("detailed")
    DETAILED
}

@Serializable
data class AgentSettings(
    val model: String? = null,
    val effort: String? = null,
    val permissions: AgentPermissions? = null,
    val output: AgentOutput? = null
)

@Serializable
data class AgentLive(
    val model: Boolean,
    val effort: Boolean,
    val permissions: List<AgentPermissions>,
    val keeps: Boolean
)
