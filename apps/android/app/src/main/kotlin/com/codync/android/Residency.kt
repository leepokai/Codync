package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import com.codync.android.core.threadNamed
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Characters counted for one stored payload, however long the row in SQLite is. */
internal const val PER_MESSAGE_CHARS = 32_000

/** Characters of preview payloads the chats that are not open may hold together. */
internal const val AGGREGATE_PREVIEW_BYTES = 480_000L

/** Characters read from the front of an oversized payload. Enough for its id and seq, not its body. */
internal const val PAYLOAD_HEAD_CHARS = 2_048

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

/** One stored payload. [text] is the whole row, or only its first [PAYLOAD_HEAD_CHARS] when [chars] exceeds [PER_MESSAGE_CHARS]. */
internal class StoredRow(val text: String, val chars: Long)

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
        StoredRow(if (chars > PER_MESSAGE_CHARS) payload.take(PAYLOAD_HEAD_CHARS) else payload, chars)
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
        val entry = boundedEntry(row.text.take(PAYLOAD_HEAD_CHARS), row.chars)?.threadNamed() ?: continue
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
 * Whole preview bots, in [ordered] order, while their counted cost fits in [budget]. A bot that does not fit is left
 * out entirely. A later bot that does fit is still kept.
 */
internal fun capPreviews(ordered: List<Pair<String, List<Entry>>>, budget: Long, cost: (Entry) -> Int): List<Entry> {
    val kept = mutableListOf<Entry>()
    var spent = 0L
    for ((_, entries) in ordered) {
        var botCost = 0L
        for (entry in entries) botCost += cost(entry).toLong()
        if (spent + botCost > budget) continue
        kept += entries
        spent += botCost
    }
    return kept
}

/** [head] is at most [PAYLOAD_HEAD_CHARS] of a payload. [fullLength] is the stored row, which is not copied. */
private fun boundedEntry(head: String, fullLength: Long): Entry? {
    val fields = headFields(head)
    val seq = fields["seq"]?.toLongOrNull() ?: return null
    return Entry(
        id = fields["id"].orEmpty(),
        seq = seq,
        botId = fields["botId"].orEmpty(),
        rev = fields["rev"]?.toLongOrNull() ?: 0L,
        kind = fields["kind"].orEmpty(),
        turn = fields["turn"]?.toLongOrNull() ?: 0L,
        data = buildJsonObject { put("text", "Large message kept in storage ($fullLength characters).") },
    )
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
