package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import com.codync.android.core.threadNamed
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
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
 * [PER_MESSAGE_CHARS]; [tail] is that row's last [PAYLOAD_TAIL_CHARS] then, else null. [routingThread] is the
 * `data.thread.id` stored beside the row by the schema that wrote it: a row's route is read, never recovered from
 * its text, so a body that mentions a thread cannot move it. [controlFlags] are the boolean fields a stub keeps,
 * read from where the row holds them (see [STUB_FLAG_KEYS]): a member written after them can push them past the head
 * and the tail alike, so they do not come from either.
 */
internal class StoredRow(
    val text: String,
    val chars: Long,
    val tail: String? = null,
    val routingThread: String? = null,
    /** The stored route could not be trusted (over the id bound, or holding an embedded NUL): no route and no actions. */
    val routingAmbiguous: Boolean = false,
    val controlFlags: Map<String, Boolean> = emptyMap(),
)

/** A row the window kept, and the characters it counted toward the budget. [routingThread] is the route the structural
 * check established for it (a stored column, a full-row scan, or a walk of the visible payload), or null for none. */
internal class KeptRow(
    val entry: Entry,
    val cost: Int,
    val routingThread: String? = null,
    val routingAmbiguous: Boolean = false,
)

/**
 * A chat's rows from its stored [newestFirst] payloads, oldest first: through the [target]-th chat row, until [budget]
 * characters are passed, its newest row always kept. A payload longer than [PER_MESSAGE_CHARS] is not decoded.
 * Hidden rows between those chat rows stay, so Trace and the working indicator still see them.
 */
internal fun newestWindow(newestFirst: Sequence<String>, target: Int, budget: Long): List<Entry> =
    windowRows(newestFirst.map { payload ->
        val chars = payload.length.toLong()
        if (chars > PER_MESSAGE_CHARS) {
            StoredRow(payload.take(PAYLOAD_HEAD_CHARS), chars, payload.takeLast(PAYLOAD_TAIL_CHARS), controlFlags = controlFlagsOf(payload))
        } else StoredRow(payload, chars)
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
        if (!oversized && !row.routingAmbiguous && row.chars <= budget - bytes) {
            // A malformed stored row must not abort the whole snapshot: keep what its head identifies as a stub.
            val read = try {
                WireJson.decodeFromString<Entry>(row.text).threadNamed()
            } catch (_: Exception) {
                boundedEntry(row.text, row.text.takeLast(PAYLOAD_TAIL_CHARS), row.chars, row.routingThread,
                    offerAnswers = false, controlFlags = row.controlFlags)
                    ?.threadNamed() ?: continue
            }
            // The decoder accepts duplicate object keys and other routing evidence beside the stored route. An approval
            // whose decoded route disagrees keeps a non-actionable stub in the owner chat instead of offering answers
            // under a thread the row does not name; every other row follows the route stored with it.
            val entry = if (read.isApproval && read.routingThreadId() != row.routingThread) {
                boundedEntry(row.text, row.text.takeLast(PAYLOAD_TAIL_CHARS), row.chars, row.routingThread,
                    offerAnswers = false, controlFlags = row.controlFlags)
                    ?.threadNamed() ?: continue
            } else read.withCheckedThread(row.routingThread)
            decoded += KeptRow(entry, row.chars.toInt(), entry.routingThreadId())
            bytes += row.chars
            if (entry.isChat) shown++
            continue
        }
        // A row already in the window means a normal row that does not fit is the budget's edge.
        if (decoded.isNotEmpty() && !oversized && !row.routingAmbiguous) break
        val entry = boundedEntry(
            row.text.take(PAYLOAD_HEAD_CHARS),
            row.tail ?: row.text.takeLast(PAYLOAD_TAIL_CHARS),
            row.chars,
            row.routingThread,
            offerAnswers = !row.routingAmbiguous,
            controlFlags = row.controlFlags,
        )?.threadNamed() ?: continue
        val cost = retainedStubChars(entry)
        if (decoded.isNotEmpty() && bytes + cost > budget) break
        decoded += KeptRow(entry, cost, entry.routingThreadId(), row.routingAmbiguous)
        bytes += cost
        if (entry.isChat) shown++
        if (!oversized && !row.routingAmbiguous) break
    }
    return decoded.reversed()
}

/**
 * [this] with `data.thread` carrying [thread] as its id, or without a `thread` when the check found none. The strict
 * walk decides where a row belongs: the decoder's own answer never adds a route back to a payload that had none.
 */
internal fun Entry.withCheckedThread(thread: String?): Entry {
    val named = threadNamed()
    val current = named.data["thread"] as? JsonObject
    val routed = when {
        thread == null -> JsonObject(named.data - "thread")
        current == null -> JsonObject(named.data + ("thread" to buildJsonObject { put("id", thread) }))
        else -> JsonObject(named.data + ("thread" to JsonObject(current + ("id" to JsonPrimitive(thread)))))
    }
    return named.copy(data = routed)
}

/** An approval row, whose answers must not be offered from a route the strict walk did not establish. */
private val Entry.isApproval: Boolean
    get() = kind == "permission"

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
 * [head] is at most [PAYLOAD_HEAD_CHARS] of a payload, [tail] its last [PAYLOAD_TAIL_CHARS] where its times sit, and
 * [storedThread] the routing metadata kept beside the row. [fullLength] is the stored row, which is not copied.
 * [controlFlags] are the boolean control fields read from the row itself, wherever it holds them.
 */
private fun boundedEntry(
    head: String,
    tail: String,
    fullLength: Long,
    storedThread: String? = null,
    /** False when the route is unproven: the stub must not carry answers. */
    offerAnswers: Boolean = true,
    controlFlags: Map<String, Boolean> = emptyMap(),
): Entry? {
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
        data = stubData(head, tail, fullLength, storedThread, offerAnswers, controlFlags),
        createdAt = times["createdAt"] ?: 0L,
        updatedAt = times["updatedAt"] ?: 0L,
    )
}

/**
 * Fields a stub keeps from the card. The large body is replaced. `toolKind` and `title` name it.
 * `status` and `options` say whether a queued row can still be sent. They do not make an approval
 * answerable: Allow and Deny wait until the full row is read. `final` and `bookkeeping` say what kind of row
 * it is: [Entry.isChat] shows a reply only when it is final, and [Entry.isWorkingStep] counts a tool only when it
 * is not the bot's bookkeeping. They are read from the whole row, not only from the head or the tail, so a large
 * member written after them cannot drop them ([STUB_FLAG_KEYS]).
 */
private val STUB_KEPT_KEYS = setOf(
    "status", "options", "selected", "clientNonce", "removed", "toolKind", "title", "final", "bookkeeping",
)

/**
 * The boolean control fields a stub keeps, and the only ones it reads from the whole row ([StoredRow.controlFlags]).
 * They are written after the body, so a large `replyTo` or `files` member after them leaves them in the unread middle:
 * neither the head nor the tail holds them. [stubFlag] reads one from where the row keeps it.
 */
internal val STUB_FLAG_KEYS = listOf("final", "bookkeeping")

/** The value of a boolean field a stub keeps, read from [text] starting at its name: `"final":true`. Null when the
 *  text does not start with one of these names, or its value is not a whole boolean. */
internal fun stubFlag(text: String): Pair<String, Boolean>? {
    val name = STUB_FLAG_KEYS.firstOrNull { text.startsWith("\"$it\"") } ?: return null
    var i = name.length + 2
    while (i < text.length && text[i].isWhitespace()) i++
    if (i >= text.length || text[i] != ':') return null
    i++
    while (i < text.length && text[i].isWhitespace()) i++
    val value = when {
        text.startsWith("true", i) -> true
        text.startsWith("false", i) -> false
        else -> return null
    }
    return name to value
}

/**
 * The [STUB_FLAG_KEYS] a whole [payload] holds, wherever it holds them. A name only appears outside a string in a
 * JSON payload (a body that mentions one has its quotes escaped), so the first mention is the field's.
 */
internal fun controlFlagsOf(payload: String): Map<String, Boolean> = STUB_FLAG_KEYS.mapNotNull { key ->
    val at = payload.indexOf("\"$key\"")
    if (at < 0) null else stubFlag(payload.substring(at, minOf(payload.length, at + FLAG_FRAGMENT_CHARS)))
}.toMap()

/** Characters of a payload a control field's name and value use, read from where the row holds it. */
internal const val FLAG_FRAGMENT_CHARS = 32

/** The fields that offer an answer. They stay off a stub whose route was not proven. */
private val ANSWER_KEYS = setOf("options", "selected")

/** A name copied only when the head holds it whole. The tail is not a source for it. */
private val HEAD_ONLY_KEYS = setOf("toolKind", "title")

/**
 * Characters a bounded stub counts. The sentence replaces the body. Each key in [STUB_KEPT_KEYS] that the stub still
 * holds adds the characters of its value, and a thread id does too. A key the stub dropped adds nothing. The sum is
 * those characters: string values as themselves, nested text the same way, and no extra allowance on top.
 */
internal fun retainedStubChars(entry: Entry): Int {
    val sentence = entry.storedStubSentence()?.length ?: entry.text.length
    val kept = STUB_KEPT_KEYS.sumOf { key -> entry.data[key]?.let(::retainedChars) ?: 0 }
    return sentence + kept + retainedThreadIdChars(entry.data)
}

/** Characters of a value the stub kept. A string counts as its text. Nested values count the same way, key names aside. */
private fun retainedChars(element: JsonElement): Int = when (element) {
    is JsonNull -> 0
    is JsonPrimitive -> element.content.length
    is JsonArray -> element.sumOf(::retainedChars)
    is JsonObject -> element.values.sumOf(::retainedChars)
}

/** Characters of `data.thread.id` when the stub still carries one. The key itself is not part of the sum. */
private fun retainedThreadIdChars(data: JsonObject): Int {
    val id = (data["thread"] as? JsonObject)?.get("id") as? JsonPrimitive ?: return 0
    return if (id.isString) id.content.length else 0
}

/**
 * The stub's `data`: the large text becomes [storedStubText]. Control fields that fit whole in [head] stay, and a
 * field that sits after the body is taken from [tail] when the head never finished it. A name is not. A cut-off value is left out.
 * [controlFlags] are the boolean fields read from the whole row, which stay whatever the row writes after them.
 */
private fun stubData(head: String, tail: String, fullLength: Long, thread: String?, offerAnswers: Boolean,
    controlFlags: Map<String, Boolean>): JsonObject {
    val fromHead = if (offerAnswers) STUB_KEPT_KEYS else STUB_KEPT_KEYS - ANSWER_KEYS
    // Answers sit with the card's fields. A value that only finishes in the tail is not copied: an unproven route's
    // options often land there, and offering them would answer under a thread nothing checked.
    val fromTail = STUB_KEPT_KEYS - ANSWER_KEYS - HEAD_ONLY_KEYS
    val kept = LinkedHashMap<String, JsonElement>()
    for ((key, value) in completeDataFields(head)) if (key in fromHead) kept[key] = value
    for ((key, value) in trailingDataFields(tail)) if (key in fromTail && key !in kept) kept[key] = value
    for ((key, flag) in controlFlags) if (key in STUB_KEPT_KEYS) kept[key] = JsonPrimitive(flag)
    return buildJsonObject {
        for ((key, value) in kept) put(key, value)
        put("text", storedStubText(fullLength))
        put("storedChars", fullLength)
        put("storedOnly", true)
        // What a thread put in its bot's chat must stay there even when its row cannot be read: the warning card
        // belongs in the affected thread, not only in the owner's main chat.
        thread?.let { put("thread", buildJsonObject { put("id", it) }) }
    }
}

/** Complete members of the top-level `data` object in [head]. A member the head cuts off is dropped, and so is the rest. */
private fun completeDataFields(head: String): Map<String, JsonElement> {
    var i = 0
    while (i < head.length && head[i].isWhitespace()) i++
    if (i >= head.length || head[i] != '{') return emptyMap()
    i++
    while (i < head.length) {
        while (i < head.length && (head[i].isWhitespace() || head[i] == ',')) i++
        if (i >= head.length || head[i] == '}') break
        if (head[i] != '"') break
        val key = readJsonString(head, i) ?: return emptyMap()
        i = key.next
        while (i < head.length && head[i].isWhitespace()) i++
        if (i >= head.length || head[i] != ':') return emptyMap()
        i++
        while (i < head.length && head[i].isWhitespace()) i++
        if (i >= head.length) return emptyMap()
        if (key.text == "data" && head[i] == '{') return readCompleteObject(head, i)
        i = skipJsonValue(head, i) ?: return emptyMap()
    }
    return emptyMap()
}

/** Complete members of the object that starts at [start], stopping at the first value [text] does not finish. */
private fun readCompleteObject(text: String, start: Int): Map<String, JsonElement> {
    val fields = LinkedHashMap<String, JsonElement>()
    var i = start + 1
    while (i < text.length) {
        while (i < text.length && (text[i].isWhitespace() || text[i] == ',')) i++
        if (i >= text.length || text[i] == '}') break
        if (text[i] != '"') break
        val key = readJsonString(text, i) ?: break
        i = key.next
        while (i < text.length && text[i].isWhitespace()) i++
        if (i >= text.length || text[i] != ':') break
        i++
        while (i < text.length && text[i].isWhitespace()) i++
        if (i >= text.length) break
        val value = readCompleteElement(text, i) ?: break
        fields[key.text] = value.element
        i = value.next
    }
    return fields
}

private class JsonSlice(val element: JsonElement, val next: Int)

/** The JSON value at [start], or null when [text] ends inside it. */
private fun readCompleteElement(text: String, start: Int): JsonSlice? {
    val end = skipJsonValue(text, start) ?: return null
    val element = try {
        WireJson.parseToJsonElement(text.substring(start, end))
    } catch (_: Exception) {
        return null
    }
    return JsonSlice(element, end)
}

/** The index just after the JSON value at [start], or null when [text] ends inside it. */
private fun skipJsonValue(text: String, start: Int): Int? = when (text.getOrNull(start)) {
    '"' -> readJsonString(text, start)?.next
    '{', '[' -> skipJsonContainer(text, start)
    else -> {
        var i = start
        while (i < text.length && text[i] != ',' && text[i] != '}' && text[i] != ']' && !text[i].isWhitespace()) i++
        if (i == start) null else i
    }
}

/**
 * Control fields that sit after a large body, read from the end of [tail] back to the first value it does not finish.
 * The times follow `data`, so the walk starts at the brace that closes `data` and never treats a time as a data field.
 */
private fun trailingDataFields(tail: String): Map<String, JsonElement> {
    val marker = tail.lastIndexOf("\"createdAt\":")
    if (marker < 0) return emptyMap()
    var i = marker
    while (i > 0 && tail[i - 1].isWhitespace()) i--
    if (i > 0 && tail[i - 1] == ',') i--
    while (i > 0 && tail[i - 1].isWhitespace()) i--
    if (i == 0 || tail[i - 1] != '}') return emptyMap()
    val found = LinkedHashMap<String, JsonElement>()
    var pos = i - 2
    while (pos >= 0) {
        while (pos >= 0 && tail[pos].isWhitespace()) pos--
        if (pos < 0) break
        val valueEnd = pos
        val valueStart = startOfValueBackward(tail, valueEnd) ?: break
        pos = valueStart - 1
        while (pos >= 0 && tail[pos].isWhitespace()) pos--
        if (pos < 0 || tail[pos] != ':') break
        pos--
        while (pos >= 0 && tail[pos].isWhitespace()) pos--
        if (pos < 0 || tail[pos] != '"') break
        val keyStart = startOfStringBackward(tail, pos) ?: break
        val key = try {
            (WireJson.parseToJsonElement(tail.substring(keyStart, pos + 1)) as? JsonPrimitive)?.contentOrNull
        } catch (_: Exception) {
            null
        } ?: break
        val element = try {
            WireJson.parseToJsonElement(tail.substring(valueStart, valueEnd + 1))
        } catch (_: Exception) {
            break
        }
        if (key in STUB_KEPT_KEYS) found[key] = element
        pos = keyStart - 1
        while (pos >= 0 && tail[pos].isWhitespace()) pos--
        if (pos < 0 || tail[pos] == '{') break
        if (tail[pos] != ',') break
        pos--
    }
    return found
}

/** The index of the value that ends at [end], or null when [text] does not hold that value whole. */
private fun startOfValueBackward(text: String, end: Int): Int? {
    if (end < 0 || end >= text.length) return null
    return when (text[end]) {
        '"' -> startOfStringBackward(text, end)
        '}', ']' -> startOfContainerBackward(text, end)
        else -> {
            var i = end
            while (i >= 0 && text[i] != ',' && text[i] != '{' && text[i] != '[' && text[i] != ':' && !text[i].isWhitespace()) i--
            if (i == end) null else i + 1
        }
    }
}

/** The opening quote of the string whose closing quote is at [endQuote]. An escaped quote stays inside the string. */
private fun startOfStringBackward(text: String, endQuote: Int): Int? {
    var i = endQuote - 1
    while (i >= 0) {
        if (text[i] == '"') {
            var slashes = 0
            var j = i - 1
            while (j >= 0 && text[j] == '\\') {
                slashes++
                j--
            }
            if (slashes % 2 == 0) return i
        }
        i--
    }
    return null
}

/** The opening brace or bracket of the container that ends at [end], or null when [text] starts inside it. */
private fun startOfContainerBackward(text: String, end: Int): Int? {
    if (text.getOrNull(end) != '}' && text.getOrNull(end) != ']') return null
    var depth = 0
    var i = end
    while (i >= 0) {
        when (text[i]) {
            '"' -> {
                i = startOfStringBackward(text, i)?.minus(1) ?: return null
                continue
            }
            '}', ']' -> depth++
            '{', '[' -> {
                depth--
                if (depth == 0) return i
            }
        }
        i--
    }
    return null
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

/** Id stored in place of a large body. Not a sentence, and not written back to SQLite. */
internal const val STORED_STUB_ID = "stored-stub"

/** Catalog key for the sentence a stub shows. `{count}` is the stored row's length. */
internal const val STORED_STUB_CAPTION = "Large message kept in storage ({count} characters)."

/** The id a stub stores. [fullLength] is kept beside it as `storedChars`, not inside the text. */
internal fun storedStubText(fullLength: Long) = STORED_STUB_ID.also { check(fullLength >= 0) }

/**
 * The English sentence a stub stands in for, or null when [this] does not carry the id and a count.
 * The budget counts this sentence. The screen translates [STORED_STUB_CAPTION] instead of storing it.
 */
internal fun Entry.storedStubSentence(): String? {
    if (text != STORED_STUB_ID) return null
    val count = (data["storedChars"] as? JsonPrimitive)?.longOrNull ?: return null
    return "Large message kept in storage ($count characters)."
}

/** True only for a row this process built instead of decoding. Never written back to SQLite. */
internal val Entry.isStoredStub: Boolean
    get() = data["storedOnly"]?.jsonPrimitive?.booleanOrNull == true

/**
 * An approval the chat kept as a stub. Status and options are copied onto it when they fit in the head. A pending card
 * is still read whole (`MirrorDatabase.answerable`) when that row fits the window budget.
 */
internal val Entry.isApprovalStub: Boolean
    get() = isStoredStub && kind == "permission"

/** A routing id is a small identifier, not a message body. Larger ids are malformed and never stored or queried. */
internal const val MAX_ROUTING_ID_CHARS = 1_024

/** The thread this entry belongs to (`data.thread.id`), stored beside its payload so routing never depends on field order. */
internal fun Entry.routingThreadId(): String? =
    ((data.threadNamed()["thread"] as? JsonObject)?.get("id") as? JsonPrimitive)
        ?.takeIf { it.isString }?.content?.takeIf { it.isNotBlank() && it.length <= MAX_ROUTING_ID_CHARS }

/** An approval stub whose stored row could not be read: it stays on screen, and says so, instead of looking answerable. */
internal val Entry.approvalReadFailed: Boolean
    get() = data[APPROVAL_READ_FAILED]?.jsonPrimitive?.booleanOrNull == true

/** [this] approval stub marked with a read failure, keeping its stored text; the marker is never written back. */
internal fun Entry.withApprovalReadFailure(): Entry =
    if (approvalReadFailed) this else copy(data = JsonObject(data + (APPROVAL_READ_FAILED to JsonPrimitive(true))))

private const val APPROVAL_READ_FAILED = "approvalReadFailed"

/**
 * Text to show for one decoded payload: the message, a tool row's output, or the patches it made. A patch-only row
 * would otherwise open an empty sheet.
 */
internal fun Entry.storedDisplayText(): String {
    if (text.isNotBlank()) return text
    fun field(key: String) = (data[key] as? JsonPrimitive)?.contentOrNull
    val output = field("output")?.takeIf { it.isNotBlank() }
    val patches = (data["diffs"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.mapNotNull(::patchText)
    val body = listOfNotNull(output, patches.joinToString("\n\n").takeIf { it.isNotEmpty() }).joinToString("\n\n")
    return body.ifEmpty { field("title").orEmpty() }
}

/** One stored file change, its path over its patch. Null for a diff that carries no lines this sheet can show. */
private fun patchText(diff: JsonObject): String? {
    val patch = (diff["patch"] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() } ?: return null
    val path = (diff["path"] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() } ?: return patch
    return "$path\n$patch"
}

/** What the walk over a payload slice expects next. */
private const val JSON_NAME = 0
private const val JSON_COLON = 1
private const val JSON_VALUE = 2
private const val JSON_SEPARATOR = 3

/**
 * [payload], the front of a stored row that a bounded read cut off, closed so that it parses: the fields it holds whole
 * are kept, a string the cut falls inside keeps the part that arrived, and the objects and arrays still open are shut.
 * A cut inside a name, a number or a literal drops what was being read, and a cut inside an escape sequence keeps the
 * string only as far as its last whole escape (`\u00` cannot be closed into a string). So what is left is always whole
 * fields. This is what lets a sheet read a bounded slice of a row that is longer than [PER_MESSAGE_CHARS] instead of
 * the whole row.
 */
internal fun closedPayloadSlice(payload: String): String {
    val open = StringBuilder()
    var expect = JSON_VALUE
    var inString = false
    var name = false
    var bare = false
    // Where a string the cut falls inside safely ends: the whole of it, or the last character before a cut escape.
    var stringEnd = payload.length
    // Where the string's last whole escape starts, so a high surrogate that lost its pair is not left behind.
    var escapeStart = -1
    // The longest front of [payload] that ends between two values, and the containers open where it ends.
    var safe = 0
    var safeOpen = ""
    fun boundary(at: Int) {
        if (at > safe) { safe = at; safeOpen = open.toString() }
    }
    var index = 0
    while (index < payload.length) {
        val c = payload[index]
        if (inString) {
            when (c) {
                '\\' -> {
                    val escape = escapeLength(payload, index)
                    if (escape == 0) {
                        // The cut landed inside this escape. `\u00` cannot stand in a string, so the value keeps the
                        // text before it and ends there.
                        stringEnd = index
                        break
                    }
                    escapeStart = index
                    index += escape
                    continue
                }
                '"' -> {
                    inString = false
                    if (name) expect = JSON_COLON else { expect = JSON_SEPARATOR; boundary(index + 1) }
                }
            }
            index++
            continue
        }
        when (c) {
            '"' -> { inString = true; name = expect == JSON_NAME; bare = false }
            '{' -> { open.append('{'); expect = JSON_NAME }
            '[' -> { open.append('['); expect = JSON_VALUE }
            ':' -> { expect = JSON_VALUE; bare = false }
            ',' -> { boundary(index); expect = if (open.lastOrNull() == '{') JSON_NAME else JSON_VALUE; bare = false }
            '}', ']' -> {
                if (open.isEmpty()) return payload
                open.deleteCharAt(open.lastIndex)
                expect = JSON_SEPARATOR
                bare = false
                boundary(index + 1)
            }
            ' ', '\n', '\r', '\t' -> if (bare) { bare = false; expect = JSON_SEPARATOR; boundary(index) }
            else -> if (expect == JSON_VALUE) bare = true
        }
        index++
    }
    // A cut inside a value's string keeps the value as far as it arrived, escapes aside; anywhere else the row's last
    // whole field is what still parses.
    if (inString && !name) {
        var end = stringEnd
        // A `\u` escape of a high surrogate that ends the string cannot be encoded back without its pair: keep the
        // text before it rather than a string the decoder refuses.
        if (escapeStart >= 0 && escapeStart + 6 == end && payload[escapeStart + 1] == 'u') {
            val code = payload.substring(escapeStart + 2, end).toIntOrNull(16)
            if (code != null && code in 0xD800..0xDBFF) end = escapeStart
        }
        return payload.substring(0, end) + '"' + closers(open)
    }
    return payload.substring(0, safe) + closers(safeOpen)
}

/**
 * Characters of the escape sequence at [start], which is a backslash: 2 for `\"` and the other one-character escapes, 6
 * for `\uXXXX`. 0 when [text] ends inside it, or its digits are not hex, so the sequence cannot stand in a string.
 */
private fun escapeLength(text: String, start: Int): Int {
    if (start + 1 >= text.length) return 0
    return when (text[start + 1]) {
        '"', '\\', '/', 'b', 'f', 'n', 'r', 't' -> 2
        'u' -> {
            if (start + 6 > text.length) return 0
            val digits = text.substring(start + 2, start + 6)
            if (digits.all { it.isDigit() || it in 'a'..'f' || it in 'A'..'F' }) 6 else 0
        }
        else -> 0
    }
}

/** The characters that shut the objects and arrays in [open], innermost first. */
private fun closers(open: CharSequence): String = buildString {
    for (index in open.indices.reversed()) append(if (open[index] == '{') '}' else ']')
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
