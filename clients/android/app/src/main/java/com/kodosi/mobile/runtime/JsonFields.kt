package com.kodosi.mobile.runtime

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.longOrNull

fun fields(vararg values: Pair<String, Any?>): JsonObject = JsonObject(
    values.associate {
        it.first to element(it.second)
    }
)

private fun element(value: Any?): JsonElement = when (value) {
    null -> JsonNull
    is JsonElement -> value
    is String -> JsonPrimitive(value)
    is Boolean -> JsonPrimitive(value)
    is Number -> JsonPrimitive(value)
    is Iterable<*> -> JsonArray(value.map(::element))
    else -> error("Unsupported command field: ${value::class.simpleName}")
}

inline fun <reified T> RuntimeEvent.value(key: String): T =
    RuntimeJson.decodeFromJsonElement(payload[key] ?: throw InvalidEvent("Missing $key in $type"))

fun JsonObject.string(key: String): String? = (get(key) as? JsonPrimitive)?.contentOrNull

fun JsonObject.number(key: String): Long? = (get(key) as? JsonPrimitive)?.longOrNull

fun JsonObject.flag(key: String): Boolean = (get(key) as? JsonPrimitive)?.booleanOrNull == true

fun JsonObject.objects(key: String): List<JsonObject> = (get(key) as? JsonArray)?.mapNotNull {
    it as? JsonObject
}.orEmpty()

fun JsonObject.strings(key: String): List<String> =
    (get(key) as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()

fun String.clippedUtf8(limit: Int): String {
    if (encodeToByteArray().size <= limit) return this
    var used = 0
    val result = StringBuilder()
    codePoints().forEachOrdered { point ->
        val chars = String(Character.toChars(point))
        val bytes = chars.encodeToByteArray().size
        if (used + bytes <= limit) {
            result.append(chars)
            used += bytes
        } else {
            used = limit
        }
    }
    return result.toString()
}
