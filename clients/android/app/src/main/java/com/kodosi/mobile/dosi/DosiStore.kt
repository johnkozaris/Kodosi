package com.kodosi.mobile.dosi

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.flag
import com.kodosi.mobile.runtime.objects
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.runtime.value
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

@Serializable
data class DosiComputer(val device: String, val name: String)

@Serializable
data class DosiItem(val id: String, val turn: String, val at: Long, val body: JsonObject)

@Serializable
data class DosiSnapshot(
    val state: String,
    val shared: Boolean,
    val items: List<DosiItem> = emptyList(),
    val earlier: Boolean = false,
    val queued: List<JsonObject> = emptyList(),
    val asks: List<JsonObject> = emptyList(),
    val last: DosiComputer? = null,
    val conversation: JsonObject? = null,
    val remote: JsonObject? = null
)

data class DosiState(
    val snapshot: DosiSnapshot? = null,
    val items: List<DosiItem> = emptyList(),
    val earlier: Boolean = false,
    val seenTurn: String? = null
) {
    val busy: Boolean get() = snapshot?.state in setOf("working", "stopping")
    val connected: Boolean get() = snapshot?.shared == true && snapshot.state != "connecting"
    val questions: List<DosiItem> get() = items.filter {
        it.body.string("kind") == "question" &&
            it.body["answer"].toString() == "null"
    }
    val badge: Int get() = questions.size.takeIf { it > 0 } ?: if (
        !busy && items.lastOrNull()?.let { it.turn != seenTurn && it.body.string("kind") != "person" } == true
    ) {
        1
    } else {
        0
    }
}

class DosiStore(private val commands: CommandSink) {
    private val mutableState = MutableStateFlow(DosiState())
    val state: StateFlow<DosiState> = mutableState
    var draft = ""
    var device: String? = null

    fun reset() {
        mutableState.value = DosiState()
        draft = ""
        device = null
    }

    fun receive(event: RuntimeEvent) {
        when (event.type) {
            "dosi.snapshot" -> {
                val next = event.value<DosiSnapshot>("dosi")
                mutableState.update { previous ->
                    val replaced = next.conversation?.string("id") != previous.snapshot?.conversation?.string("id")
                    val first = next.items.firstOrNull()?.id
                    val start = previous.items.indexOfFirst { it.id == first }
                    previous.copy(
                        snapshot = next,
                        items = if (!replaced && start >= 0) previous.items.take(start) + next.items else next.items,
                        earlier = if (!replaced && start >= 0) previous.earlier else next.earlier,
                        seenTurn = if (previous.snapshot == null && next.state !in setOf("working", "stopping")) {
                            next.items.lastOrNull()?.turn
                        } else {
                            previous.seenTurn
                        }
                    )
                }
            }

            "dosi.item" -> {
                val item = event.value<DosiItem>("item")
                mutableState.update {
                    val index = it.items.indexOfFirst { old -> old.id == item.id }
                    it.copy(
                        items = if (index <
                            0
                        ) {
                            it.items + item
                        } else {
                            it.items.toMutableList().apply { set(index, item) }
                        }
                    )
                }
            }
        }
    }

    fun see() = mutableState.update { it.copy(seenTurn = it.items.lastOrNull()?.turn) }

    suspend fun send(text: String, computer: DosiComputer) {
        commands.request("dosi.send", fields("text" to text.trim(), "device" to computer.device))
        if (draft == text) draft = ""
    }

    fun stop() = commands.perform("dosi.stop")

    suspend fun answer(item: DosiItem, choices: List<String>) {
        commands.request("dosi.answer", fields("itemId" to item.id, "choices" to choices))
    }

    suspend fun renew() {
        commands.request("dosi.new")
    }

    fun takeBack(id: String) = commands.perform("dosi.unqueue", fields("queuedId" to id))

    suspend fun earlier() {
        val first = state.value.items.firstOrNull()?.id ?: return
        val page = commands.request("dosi.history", fields("before" to first)).value<JsonObject>("page")
        if (state.value.items.firstOrNull()?.id != first) return
        val items = page.objects("items").map {
            com.kodosi.mobile.runtime.RuntimeJson.decodeFromJsonElement(DosiItem.serializer(), it)
        }
        mutableState.update { old ->
            old.copy(
                items = items.filter { item ->
                    old.items.none { it.id == item.id }
                } + old.items,
                earlier = page.flag("earlier")
            )
        }
    }
}
