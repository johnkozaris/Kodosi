package com.kodosi.mobile.runtime

import java.security.SecureRandom
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull

val RuntimeJson = Json { ignoreUnknownKeys = true }

class InvalidEvent(message: String) : Exception(message)

/** An event of the runtime: its type, the account it belongs to, and its fields. */
class RuntimeEvent(val type: String, val accountUserId: String?, val accountEpoch: Long, val payload: JsonObject) {
    fun string(key: String): String? = (payload[key] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull

    fun bool(key: String): Boolean? = (payload[key] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull

    operator fun get(key: String): JsonElement? = payload[key]

    /** Whether this is the failure of a refresh that the app asks for by itself. It is no news to
     *  the person: the runtime refreshes again when the connection is back. */
    val failedRefresh: Boolean
        get() = type.endsWith(".error") && string("operation") in REFRESHES

    companion object {
        private val REFRESHES =
            setOf(
                "session.list",
                "auth.refresh",
                "friends.refresh",
                "devices.refresh",
                "mission.list"
            )
        fun parse(bytes: ByteArray): RuntimeEvent {
            val fields = RuntimeJson.parseToJsonElement(bytes.decodeToString()) as? JsonObject
                ?: throw InvalidEvent("an event is not an object")
            val type = (fields["type"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            if (type.isNullOrEmpty() || !fields.containsKey("accountUserId")) {
                throw InvalidEvent("an event has no type or account")
            }
            val user = when (val value = fields["accountUserId"]) {
                is JsonNull -> null
                is JsonPrimitive -> value.takeIf { it.isString }?.content ?: throw InvalidEvent("invalid account")
                else -> throw InvalidEvent("invalid account")
            }
            val epoch = (fields["accountEpoch"] as? JsonPrimitive)?.takeIf { !it.isString }?.longOrNull
                ?: throw InvalidEvent("an event has no account epoch")
            return RuntimeEvent(type, user, epoch, fields)
        }
    }
}

object UuidV7 {
    private val random = SecureRandom()

    fun generate(now: Long = System.currentTimeMillis()): String {
        val bytes = ByteArray(16)
        random.nextBytes(bytes)
        for (index in 0 until 6) {
            bytes[index] = (now ushr (40 - 8 * index)).toByte()
        }
        bytes[6] = ((bytes[6].toInt() and 0x0F) or 0x70).toByte()
        bytes[8] = ((bytes[8].toInt() and 0x3F) or 0x80).toByte()
        val hex = bytes.joinToString("") { "%02x".format(it) }
        return "${hex.substring(0, 8)}-${hex.substring(8, 12)}-${hex.substring(12, 16)}-" +
            "${hex.substring(16, 20)}-${hex.substring(20)}"
    }
}

/** The sentences for people that the stores need, in the app's language. */
interface Words {
    val unavailable: String
    val didNotWork: String
    val accountChanged: String
    val otherAccount: String
    val signInEnded: String
    val accountDeleted: String

    fun rejected(code: Int): String
}
