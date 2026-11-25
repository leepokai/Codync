package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import com.codync.android.core.threadNamed
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Characters counted for one stored payload, however long the row in SQLite is. */
internal const val PER_MESSAGE_CHARS = 32_000

/** Characters of preview payloads the chats that are not open may hold together. */
internal const val AGGREGATE_PREVIEW_BYTES = 480_000L

/** Characters read from the front of an oversized payload. Enough for its id and seq, not its body. */
internal const val PAYLOAD_HEAD_CHARS = 2_048

/** Characters read from the end of an oversized payload: its times sit after the body. */
internal const val PAYLOAD_TAIL_CHARS = 128

/**
 * How much of each chat [MirrorDatabase.snapshot] loads. The open chat keeps its
 * newest [window] chat rows ([com.codync.android.core.Entry.isChat]), more while the reader pages back
 * (see [windows]) up to [MAX_WINDOW]. Tool and thought rows between those chat rows come along, so they do not
 * use up the quota; the read still stops after a bounded multiple of [window] and never loads a chat whole.
 * Every other chat keeps its newest [preview] rows. [previewPriority] names those chats newest first.
 * Older rows stay in SQLite and are read on demand.
 */
data class Residency(
    val full: Set<String>,
    val preview: Int = 8,
    val window: Int = DEFAULT_WINDOW,
    val windows: Map<String, Int> = emptyMap(),
    val previewPriority: List<String> = emptyList(),
) {
    fun limit(botId: String): Int = (windows[botId] ?: window).coerceIn(window, maxOf(window, MAX_WINDOW))

    /**
     * [botId]'s window once older rows reached the phone, from SQLite or a page from the computer: a page further back,
     * whatever those rows hold, so a page of only tool or thought rows moves the chat too.
     */
    fun widened(botId: String): Int = limit(botId) + PAGE

    companion object {
        const val DEFAULT_WINDOW = 80
        /** Rows a chat's window grows by each time the reader pages back. */
        const val PAGE = 80
        /** The most rows one chat holds in memory, however far the reader has paged back. */
        const val MAX_WINDOW = 400

        /** Whether the open chat should ask for older rows. [shown] counts rows the chat list shows, not tool or thought rows. */
        fun needsEarlier(shown: Int, complete: Boolean): Boolean = !complete && shown < MAX_WINDOW
    }
}

/**
 * One stored payload. [text] is the whole row, or only its first [PAYLOAD_HEAD_CHARS] when [chars] exceeds
 * [PER_MESSAGE_CHARS]; [tail] is that row's last [PAYLOAD_TAIL_CHARS] then, else null.
 */
internal class StoredRow(val text: String, val chars: Long, val tail: String? = null)

/** A row the window kept, and the characters it counted toward the budget. */
internal class KeptRow(val entry: Entry, val cost: Int)

/**
 * A chat's rows from its stored [newestFirst] payloads, oldest first: through the [target]-th chat row, until [budget]
 * characters are passed, its newest row always kept. A payload longer than [PER_MESSAGE_CHARS] is not decoded.
 * Hidden rows between those chat rows stay, so Trace and the working indicator still see them.
 */
internal fun newestWindow(newestFirst: Sequence<String>, target: Int, budget: Long): List<Entry> =
    windowRows(newestFirst.map { payload ->
        val chars = payload.length.toLong()
        if (chars > PER_MESSAGE_CHARS) StoredRow(payload.take(PAYLOAD_HEAD_CHARS), chars, payload.takeLast(PAYLOAD_TAIL_CHARS))
        else StoredRow(payload, chars)
    }, target, budget).map { it.entry }

/**
 * [newestWindow] for rows whose full length is already known. A payload longer than [PER_MESSAGE_CHARS] is not
 * decoded: the window keeps a short entry and counts only that entry's text, so older rows can still load.
 * A normal row that does not fit stops the walk. [seq] missing from an oversized head drops that row.
 */
internal fun windowRows(newestFirst: Sequence<StoredRow>, target: Int, budget: Long): List<KeptRow> {
    val decoded = mutableListOf<KeptRow>()
    var shown = 0
    var bytes = 0L
    val rows = newestFirst.iterator()
    while (shown < target && bytes < budget && rows.hasNext()) {
        val row = rows.next()
        val oversized = row.chars > PER_MESSAGE_CHARS
        if (!oversized && row.chars <= budget - bytes) {
            // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0 (rows an earlier app saved from one).
            val entry = WireJson.decodeFromString<Entry>(row.text).threadNamed()
            decoded += KeptRow(entry, row.chars.toInt())
            bytes += row.chars
            if (entry.isChat) shown++
            continue
        }
        // A row already in the window means a normal row that does not fit is the budget's edge.
        if (decoded.isNotEmpty() && !oversized) break
        val entry = boundedEntry(row.text.take(PAYLOAD_HEAD_CHARS), row.tail ?: row.text.takeLast(PAYLOAD_TAIL_CHARS), row.chars)
            ?.threadNamed() ?: continue
        val cost = entry.text.length
        if (decoded.isNotEmpty() && bytes + cost > budget) break
        decoded += KeptRow(entry, cost)
        bytes += cost
        if (entry.isChat) shown++
        if (!oversized) break
    }
    return decoded.reversed()
}

/**
 * The chats to preview, in order: those the reader opened ([priority]) first, then the rest.
 */
internal fun previewOrder(priority: List<String>, chats: List<String>): List<String> {
    val known = chats.toSet()
    val ordered = LinkedHashSet<String>(chats.size)
    for (id in priority) if (id in known) ordered += id
    ordered += chats
    return ordered.toList()
}

/**
 * Previews for [order]'s chats while their counted cost fits in [budget]. [read] builds one chat's rows when it is
 * asked for them, and one that does not fit is dropped at once: no preview the budget refused is held with the ones
 * it kept, so what the previews cost together stays the budget plus one chat's preview.
 */
internal fun previewsWithin(order: List<String>, budget: Long, read: (String) -> List<KeptRow>): List<Entry> {
    val kept = mutableListOf<Entry>()
    var spent = 0L
    for (chat in order) {
        val rows = read(chat)
        val cost = rows.sumOf { it.cost.toLong() }
        if (spent + cost > budget) continue
        spent += cost
        for (row in rows) kept += row.entry
    }
    return kept
}

/**
 * [head] is at most [PAYLOAD_HEAD_CHARS] of a payload, [tail] its last [PAYLOAD_TAIL_CHARS] where its times sit.
 * [fullLength] is the stored row, which is not copied.
 */
private fun boundedEntry(head: String, tail: String, fullLength: Long): Entry? {
    val fields = headFields(head)
    val seq = fields["seq"]?.toLongOrNull() ?: return null
    val times = tailTimes(tail)
    return Entry(
        id = fields["id"].orEmpty(),
        seq = seq,
        botId = fields["botId"].orEmpty(),
        rev = fields["rev"]?.toLongOrNull() ?: 0L,
        kind = fields["kind"].orEmpty(),
        turn = fields["turn"]?.toLongOrNull() ?: 0L,
        data = buildJsonObject {
            put("text", storedStubText(fullLength))
            put("storedOnly", true)
        },
        createdAt = times["createdAt"] ?: 0L,
        updatedAt = times["updatedAt"] ?: 0L,
    )
}

/** The times a payload's last [PAYLOAD_TAIL_CHARS] hold, written after its body. Each is taken from its last mention. */
private fun tailTimes(tail: String): Map<String, Long> {
    val times = HashMap<String, Long>()
    for (key in listOf("createdAt", "updatedAt")) {
        val at = tail.lastIndexOf("\"$key\":")
        if (at < 0) continue
        val start = at + key.length + 3
        var end = start
        while (end < tail.length && tail[end].isDigit()) end++
        if (end > start) tail.substring(start, end).toLongOrNull()?.let { times[key] = it }
    }
    return times
}

/** The sentence a resident stub shows. The full payload stays in SQLite. */
internal fun storedStubText(fullLength: Long) = "Large message kept in storage ($fullLength characters)."

/** True only for a row this process built instead of decoding. Never written back to SQLite. */
internal val Entry.isStoredStub: Boolean
    get() = data["storedOnly"]?.jsonPrimitive?.booleanOrNull == true

/**
 * An approval the chat kept as a stub. Its options and status sit past the head, so the card must read the row whole
 * (`MirrorDatabase.answerable`) before anything answers it.
 */
internal val Entry.isApprovalStub: Boolean
    get() = isStoredStub && kind == "permission"

/** Text to show for one decoded payload: the message, or a tool row's output. */
internal fun Entry.storedDisplayText(): String {
    if (text.isNotBlank()) return text
    fun field(key: String) = (data[key] as? JsonPrimitive)?.contentOrNull
    return field("output")?.takeIf { it.isNotBlank() } ?: field("title").orEmpty()
}

/** Top-level JSON fields that [head] holds in full. A value cut off at the end is left out. */
private fun headFields(head: String): Map<String, String> {
    val fields = LinkedHashMap<String, String>()
    var i = 0
    while (i < head.length && head[i].isWhitespace()) i++
    if (i >= head.length || head[i] != '{') return fields
    i++
    while (i < head.length) {
        while (i < head.length && (head[i].isWhitespace() || head[i] == ',')) i++
        if (i >= head.length || head[i] == '}') break
        if (head[i] != '"') break
        val key = readJsonString(head, i) ?: return fields
        i = key.next
        while (i < head.length && head[i].isWhitespace()) i++
        if (i >= head.length || head[i] != ':') return fields
        i++
        while (i < head.length && head[i].isWhitespace()) i++
        if (i >= head.length) return fields
        when (head[i]) {
            '"' -> {
                val value = readJsonString(head, i) ?: return fields
                fields[key.text] = value.text
                i = value.next
            }
            '{', '[' -> {
                val end = skipJsonContainer(head, i) ?: return fields
                i = end
            }
            else -> {
                val start = i
                while (i < head.length && head[i] != ',' && head[i] != '}' && !head[i].isWhitespace()) i++
                if (i == start) return fields
                fields[key.text] = head.substring(start, i)
            }
        }
    }
    return fields
}

private class JsonToken(val text: String, val next: Int)

private fun readJsonString(text: String, start: Int): JsonToken? {
    if (start >= text.length || text[start] != '"') return null
    val out = StringBuilder()
    var i = start + 1
    while (i < text.length) {
        val c = text[i]
        if (c == '"') return JsonToken(out.toString(), i + 1)
        if (c != '\\') {
            out.append(c)
            i++
            continue
        }
        if (i + 1 >= text.length) return null
        when (val escaped = text[i + 1]) {
            '"', '\\', '/' -> { out.append(escaped); i += 2 }
            'n' -> { out.append('\n'); i += 2 }
            't' -> { out.append('\t'); i += 2 }
            'r' -> { out.append('\r'); i += 2 }
            'u' -> {
                if (i + 6 > text.length) return null
                val code = text.substring(i + 2, i + 6).toIntOrNull(16) ?: return null
                out.append(code.toChar())
                i += 6
            }
            else -> return null
        }
    }
    return null
}

/** The index just after the object or array that starts at [start], or null when [text] ends inside it. */
private fun skipJsonContainer(text: String, start: Int): Int? {
    var depth = 0
    var inString = false
    var escaped = false
    var i = start
    while (i < text.length) {
        val c = text[i]
        if (inString) {
            if (escaped) escaped = false
            else if (c == '\\') escaped = true
            else if (c == '"') inString = false
        } else when (c) {
            '"' -> inString = true
            '{', '[' -> depth++
            '}', ']' -> {
                depth--
                if (depth == 0) return i + 1
            }
        }
        i++
    }
    return null
}
