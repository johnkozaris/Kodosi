package com.kodosi.mobile.sharing

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.fields
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable

/** A change that lets someone new in and waits for the owner's OK, as the runtime words it. */
@Serializable
data class SharingApproval(val approvalId: String, val title: String, val message: String, val confirm: String)

/** The open questions in the order they came; the app shows the first one. */
class SharingApprovals(private val commands: CommandSink) {
    val waiting: StateFlow<List<SharingApproval>>
        field = MutableStateFlow(emptyList())

    fun receive(event: RuntimeEvent) {
        when (event.type) {
            "sharing.approval" -> {
                val asked = RuntimeJson.decodeFromJsonElement(SharingApproval.serializer(), event.payload)
                waiting.update { open -> if (open.any { it.approvalId == asked.approvalId }) open else open + asked }
            }

            "sharing.approvalEnded" -> event.string("approvalId")?.let(::drop)
        }
    }

    fun answer(approval: SharingApproval, allow: Boolean) {
        if (waiting.value.none { it.approvalId == approval.approvalId }) return
        drop(approval.approvalId)
        commands.perform("sharing.answer", fields("approvalId" to approval.approvalId, "allow" to allow))
    }

    fun reset() {
        waiting.value = emptyList()
    }

    private fun drop(approvalId: String) = waiting.update { open -> open.filterNot { it.approvalId == approvalId } }
}
