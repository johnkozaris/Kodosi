package com.kodosi.mobile.canvas

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeFailure
import com.kodosi.mobile.runtime.UuidV7
import com.kodosi.mobile.runtime.Words
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.runtime.number
import com.kodosi.mobile.runtime.string
import com.kodosi.mobile.runtime.value
import com.kodosi.mobile.terminals.RuntimeSession
import java.io.ByteArrayOutputStream
import java.util.Base64
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.json.JsonObject

class CanvasStore(private val commands: CommandSink, private val words: Words) {
    private data class Cached(val session: String, val key: String, val revision: Long)

    private val cache = LinkedHashMap<Cached, ByteArray>(16, 0.75f, true)
    private val links = mutableSetOf<DocumentLink>()
    private val opened = mutableMapOf<String, Long>()
    private val started = System.currentTimeMillis()
    private val mutableNews = MutableStateFlow<Map<String, String>>(emptyMap())
    val news: StateFlow<Map<String, String>> = mutableNews
    private var generation = 0

    fun reset() {
        generation++
        links.toList().forEach(DocumentLink::close)
        cache.clear()
        opened.clear()
        mutableNews.value = emptyMap()
    }

    fun observe(sessions: List<RuntimeSession>) {
        val present = sessions.map { it.id }.toSet()
        cache.keys.removeAll { it.session !in present }
        opened.keys.retainAll(present)
        mutableNews.value = sessions.mapNotNull { session ->
            val show = session.canvas?.latestNotice ?: return@mapNotNull null
            if (show.at > (opened[session.id] ?: started)) session.id to show.key else null
        }.toMap()
    }

    fun saw(session: RuntimeSession) {
        session.canvas?.latestNotice?.let { opened[session.id] = it.at }
        mutableNews.value = mutableNews.value - session.id
    }

    fun newShow(session: RuntimeSession): CanvasOpen? =
        session.canvas?.open?.takeIf { it.at > (opened[session.id] ?: started) }

    fun receive(event: RuntimeEvent) {
        links.toList().forEach { it.receive(event) }
    }

    fun link(session: String, key: String): DocumentLink = DocumentLink(commands, session, key) {
        links.remove(it)
    }
        .also { links.add(it) }

    suspend fun ask(session: String, request: JsonObject): JsonObject =
        commands.request("canvas.request", fields("sessionId" to session, "request" to request)).value("answer")

    fun stored(session: String, item: CanvasItem): ByteArray? = cache[Cached(session, item.key, item.revision)]

    suspend fun content(session: String, item: CanvasItem, progress: (Float) -> Unit = {}): ByteArray {
        val cached = Cached(session, item.key, item.revision)
        cache[cached]?.let { return it }
        val epoch = generation
        var revision = item.revision
        repeat(4) {
            val result = ByteArrayOutputStream()
            var total: Long? = null
            while (total == null || result.size().toLong() < total) {
                val answer = ask(
                    session,
                    fields(
                        "type" to "read",
                        "key" to item.key,
                        "revision" to revision,
                        "offset" to result.size()
                    )
                )
                if (epoch != generation) throw RuntimeFailure(words.accountChanged)
                if (answer.string("answer") == "stale") {
                    revision = answer.number("revision") ?: throw RuntimeFailure(words.didNotWork)
                    break
                }
                val size = answer.number("total") ?: throw RuntimeFailure(words.didNotWork)
                val part = answer.string("data")?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
                    ?: throw RuntimeFailure(words.didNotWork)
                if (answer.string("answer") != "part" || answer.string("key") != item.key ||
                    answer.number("revision") != revision || answer.number("offset") != result.size().toLong() ||
                    size !in 0..MAX_FILE_BYTES.toLong() || result.size() + part.size > size ||
                    (part.isEmpty() && result.size().toLong() != size) || (total != null && total != size)
                ) {
                    throw RuntimeFailure(words.didNotWork)
                }
                total = size
                result.write(part)
                progress(if (size == 0L) 1f else result.size().toFloat() / size)
                if (result.size().toLong() == total) {
                    val bytes = result.toByteArray()
                    if (epoch != generation) throw RuntimeFailure(words.accountChanged)
                    cache[Cached(session, item.key, revision)] = bytes
                    while (cache.values.sumOf { it.size.toLong() } > MAX_CACHE_BYTES && cache.size > 1) {
                        cache.remove(cache.keys.first())
                    }
                    return bytes
                }
            }
        }
        throw RuntimeFailure(words.didNotWork)
    }

    suspend fun attach(
        session: String,
        name: String,
        bytes: ByteArray,
        page: Boolean = false,
        progress: (Float) -> Unit = {
        }
    ): String? {
        require(bytes.size <= MAX_FILE_BYTES)
        val epoch = generation
        val upload = UuidV7.generate()
        var offset = 0
        var key: String?
        do {
            if (epoch != generation) throw RuntimeFailure(words.accountChanged)
            val end = minOf(offset + PART_BYTES, bytes.size)
            key = ask(
                session,
                fields(
                    "type" to "attach",
                    "upload" to upload,
                    "name" to name,
                    "offset" to offset,
                    "data" to Base64.getEncoder().encodeToString(bytes.copyOfRange(offset, end)),
                    "last" to (end == bytes.size),
                    "page" to page
                )
            ).string("key")
            if (epoch != generation) throw RuntimeFailure(words.accountChanged)
            offset = end
            progress(if (bytes.isEmpty()) 1f else offset.toFloat() / bytes.size)
        } while (offset < bytes.size)
        return key
    }

    suspend fun remove(session: String, key: String) {
        ask(session, fields("type" to "remove", "keys" to listOf(key)))
        cache.keys.removeAll { it.session == session && it.key == key }
    }

    suspend fun wake(session: String) {
        ask(session, fields("type" to "act", "action" to fields("do" to "wake")))
    }

    companion object {
        const val MAX_FILE_BYTES = 32 * 1024 * 1024
        const val PART_BYTES = 256 * 1024
        private const val MAX_CACHE_BYTES = 64 * 1024 * 1024
    }
}
