package com.kodosi.mobile.runtime

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

object TestWords : Words {
    override val unavailable = "unavailable"
    override val didNotWork = "did not work"
    override val accountChanged = "account changed"
    override val otherAccount = "other account"
    override val signInEnded = "sign-in ended"
    override val accountDeleted = "account deleted"

    override fun rejected(code: Int) = "rejected $code"
}

fun event(json: String): RuntimeEvent = RuntimeEvent.parse(json.encodeToByteArray())

/** An event of the runtime outside any account, with the given fields after its type. */
fun event(type: String, fields: String): RuntimeEvent =
    event("""{"type":"$type","accountUserId":null,"accountEpoch":0${if (fields.isEmpty()) "" else ",$fields"}}""")

/** The answer of the runtime to a request. */
fun answer(type: String, requestId: String, operation: String, fields: String = ""): RuntimeEvent =
    event(type, """"requestId":"$requestId","operation":"$operation"${if (fields.isEmpty()) "" else ",$fields"}""")

/** A command sink whose commands land in a list instead of a runtime. */
class SentCommands(var result: Int = Ffi.OK) {
    val commands = mutableListOf<JsonObject>()
    val sink = CommandSink(TestWords).also { sink ->
        sink.transport = { bytes ->
            commands += RuntimeJson.parseToJsonElement(bytes.decodeToString()).jsonObject
            result
        }
    }

    val types: List<String> get() = commands.map { it.string("type") }

    fun last(type: String): JsonObject = commands.last { it.string("type") == type }
}

fun JsonObject.string(key: String): String = (this[key] as JsonPrimitive).content
