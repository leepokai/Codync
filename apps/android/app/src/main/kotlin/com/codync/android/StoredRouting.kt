package com.codync.android

import android.database.sqlite.SQLiteDatabase

/**
 * Routing for a cached row written before `thread_id`: the `id` of `data.thread`, or null when the row has no genuine
 * thread marker. The payload is read in SQLite-sized pieces and walked as JSON, so a large body, a field after it, a
 * nested `thread` in some other object, or a different order cannot change where the row belongs. No text search is
 * trusted. The row itself may be cut off after a complete thread object: the marker stays usable, while a truncated
 * thread object, duplicate direct ids, duplicate direct thread fields, a container closed by the wrong bracket or a
 * trailing comma stay ambiguous.
 */
internal fun storedRoutingThread(db: SQLiteDatabase, id: String): String? = rootThread(PayloadReader(db, id), requireClose = false)

/**
 * The direct `data.thread.id` visible in an already-read payload fragment. A fragment may stop after the thread object,
 * so a missing root close is allowed; duplicate direct ids, an unclosed thread object and nesting are still rejected.
 */
internal fun routingThreadIn(payload: String): String? = rootThread(TextReader(payload), requireClose = false)

/** A routing id is a small identifier, not a message body. Larger ids are malformed and never stored or queried. */
internal const val MAX_ROUTING_ID_CHARS = 1_024

/** Field names and nesting are small. A larger key is malformed input, not a reason to buffer it. */
private const val MAX_ROUTING_KEY_CHARS = 256
private const val MAX_JSON_DEPTH = 256

/** How many characters one routing read asks SQLite for: well below its cursor window, while few round trips for a large row. */
private const val ROUTING_CHUNK = 65_536

private interface CharReader {
    fun peek(): Char?
    fun take(): Char?
}

/**
 * The payload as a stream. SQLite counts characters here, so [offset] advances by the code points a chunk held, not by
 * its UTF-16 length; a surrogate pair therefore never makes the next chunk skip or repeat a boundary.
 */
private class PayloadReader(private val db: SQLiteDatabase, private val id: String) : CharReader {
    private var offset = 1L
    private var chunk = ""
    private var index = 0
    private var ended = false

    override fun peek(): Char? {
        while (index >= chunk.length) {
            if (ended) return null
            chunk = load()
            index = 0
            if (chunk.isEmpty()) {
                ended = true
                return null
            }
        }
        return chunk[index]
    }

    override fun take(): Char? = peek()?.also { index++ }

    private fun load(): String {
        val text = db.rawQuery(
            "SELECT substr(payload, ?, ?) FROM entries WHERE id=? LIMIT 1",
            arrayOf(offset.toString(), ROUTING_CHUNK.toString(), id),
        ).use { if (it.moveToFirst()) it.getString(0).orEmpty() else "" }
        if (text.isNotEmpty()) offset += text.codePointCount(0, text.length)
        return text
    }
}

private class TextReader(private val text: String) : CharReader {
    private var index = 0
    override fun peek(): Char? = text.getOrNull(index)
    override fun take(): Char? = text.getOrNull(index++)
}

/**
 * [data][thread][id] directly under the root entry object. Every root field is walked through its closing brace when
 * [requireClose] is true, so a duplicate `data` cannot be hidden later. A visible fragment may stop after the thread
 * object; then only complete fields before its end are evidence.
 */
private fun rootThread(reader: CharReader, requireClose: Boolean): String? {
    skipSpace(reader)
    if (reader.take() != '{') return null
    var dataSeen = false
    var found: String? = null
    // A comma promises another field: an object that closes instead is malformed, not a fragment that ended.
    var fieldDue = false
    while (true) {
        skipSpace(reader)
        when (reader.peek()) {
            null -> return if (requireClose) null else found
            '}' -> {
                if (fieldDue) return null
                reader.take()
                return found
            }
            '"' -> Unit
            else -> return null
        }
        fieldDue = false
        val key = readString(reader, MAX_ROUTING_KEY_CHARS) ?: return null
        skipSpace(reader)
        if (reader.take() != ':') return null
        skipSpace(reader)
        if (key == "data") {
            if (dataSeen) return null
            dataSeen = true
            if (reader.peek() != '{') return null
            found = directThread(reader, requireClose)
        } else if (!skipValue(reader)) {
            return null
        }
        when (nextField(reader)) {
            true -> fieldDue = true
            false -> return found
            null -> return if (!requireClose && reader.peek() == null) found else null
        }
    }
}

/** A direct `thread` of [data][reader], skipping every value nested in another field. The data object must stay unambiguous. */
private fun directThread(reader: CharReader, requireClose: Boolean): String? {
    if (reader.take() != '{') return null
    var threadSeen = false
    var found: String? = null
    var fieldDue = false
    while (true) {
        skipSpace(reader)
        when (reader.peek()) {
            null -> return if (requireClose) null else found
            '}' -> {
                if (fieldDue) return null
                reader.take()
                return found
            }
            '"' -> Unit
            else -> return null
        }
        fieldDue = false
        val key = readString(reader, MAX_ROUTING_KEY_CHARS) ?: return null
        skipSpace(reader)
        if (reader.take() != ':') return null
        skipSpace(reader)
        if (key == "thread") {
            // A second direct thread is ambiguous even when the first one had a valid id.
            if (threadSeen) return null
            threadSeen = true
            if (reader.peek() != '{') return null
            found = threadObjectId(reader)
        } else if (!skipValue(reader)) {
            return null
        }
        when (nextField(reader)) {
            true -> fieldDue = true
            false -> return found
            null -> return if (!requireClose && reader.peek() == null) found else null
        }
    }
}

/**
 * The one non-blank string `id` of [thread][reader], never an id from an object nested inside it. The whole object must
 * be present: a row cut off after its first id is ambiguous, as is a thread that names two direct ids or a non-string
 * id. A multi-megabyte id is rejected while it is being read, not accumulated.
 */
private fun threadObjectId(reader: CharReader): String? {
    if (reader.take() != '{') return null
    var idSeen = false
    var found: String? = null
    var fieldDue = false
    while (true) {
        skipSpace(reader)
        when (reader.peek()) {
            null -> return null
            '}' -> {
                if (fieldDue) return null
                reader.take()
                return found
            }
            '"' -> Unit
            else -> return null
        }
        fieldDue = false
        val key = readString(reader, MAX_ROUTING_KEY_CHARS) ?: return null
        skipSpace(reader)
        if (reader.take() != ':') return null
        skipSpace(reader)
        if (key == "id") {
            if (idSeen) return null
            idSeen = true
            found = readString(reader, MAX_ROUTING_ID_CHARS)?.takeIf { it.isNotBlank() } ?: return null
        } else if (!skipValue(reader)) {
            return null
        }
        fieldDue = when (nextField(reader)) {
            true -> true
            false -> return found
            null -> return null
        }
    }
}

/** Consumes a comma and returns true, or the object's closing brace and returns false; null is malformed input. */
private fun nextField(reader: CharReader): Boolean? {
    skipSpace(reader)
    return when (reader.peek()) {
        ',' -> { reader.take(); true }
        '}' -> { reader.take(); false }
        else -> null
    }
}

private fun skipSpace(reader: CharReader) {
    while (reader.peek()?.isWhitespace() == true) reader.take()
}

/**
 * One JSON string, decoded enough for field names and ids. The surrounding quotes are consumed. A string longer than
 * [maxChars] is rejected instead of buffered; its caller then stops treating the row as routed.
 */
private fun readString(reader: CharReader, maxChars: Int): String? {
    skipSpace(reader)
    if (reader.take() != '"') return null
    val out = StringBuilder()
    while (true) {
        val c = reader.take() ?: return null
        when (c) {
            '"' -> return out.toString()
            '\\' -> when (val escaped = reader.take() ?: return null) {
                '"', '\\', '/' -> appendBounded(out, escaped, maxChars) ?: return null
                'b' -> appendBounded(out, '\b', maxChars) ?: return null
                'f' -> appendBounded(out, '\u000C', maxChars) ?: return null
                'n' -> appendBounded(out, '\n', maxChars) ?: return null
                'r' -> appendBounded(out, '\r', maxChars) ?: return null
                't' -> appendBounded(out, '\t', maxChars) ?: return null
                'u' -> {
                    var code = 0
                    repeat(4) {
                        val digit = reader.take()?.digitToIntOrNull(16) ?: return null
                        code = code * 16 + digit
                    }
                    appendBounded(out, code.toChar(), maxChars) ?: return null
                }
                else -> return null
            }
            else -> appendBounded(out, c, maxChars) ?: return null
        }
    }
}

private fun appendBounded(out: StringBuilder, c: Char, maxChars: Int): Unit? {
    if (out.length >= maxChars) return null
    out.append(c)
    return Unit
}

/** Skips a string without copying a large body. */
private fun skipString(reader: CharReader): Boolean {
    if (reader.take() != '"') return false
    var escaped = false
    while (true) {
        val c = reader.take() ?: return false
        if (escaped) escaped = false
        else if (c == '\\') escaped = true
        else if (c == '"') return true
    }
}

/** Skips one complete value, including arbitrarily large strings and nested objects. */
private fun skipValue(reader: CharReader): Boolean {
    skipSpace(reader)
    return when (reader.peek()) {
        '"' -> skipString(reader)
        '{', '[' -> skipContainer(reader)
        null -> false
        else -> {
            var c = reader.peek() ?: return true
            while (c != ',' && c != '}' && c != ']' && !c.isWhitespace()) {
                reader.take()
                c = reader.peek() ?: return true
            }
            true
        }
    }
}

private fun skipContainer(reader: CharReader): Boolean {
    val open = reader.take() ?: return false
    if (open != '{' && open != '[') return false
    // The closing character each open container still needs: `}` never closes `[`, and the depth stays bounded.
    val closers = StringBuilder().append(if (open == '{') '}' else ']')
    var inString = false
    var escaped = false
    while (closers.isNotEmpty()) {
        val c = reader.take() ?: return false
        if (inString) {
            if (escaped) escaped = false
            else if (c == '\\') escaped = true
            else if (c == '"') inString = false
            continue
        }
        when (c) {
            '"' -> inString = true
            '{' -> {
                closers.append('}')
                if (closers.length > MAX_JSON_DEPTH) return false
            }
            '[' -> {
                closers.append(']')
                if (closers.length > MAX_JSON_DEPTH) return false
            }
            '}', ']' -> {
                if (closers[closers.length - 1] != c) return false
                closers.setLength(closers.length - 1)
            }
        }
    }
    return true
}
