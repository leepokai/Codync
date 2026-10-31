package com.codync.android.core

import java.time.Instant
import java.time.ZoneId
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

// When to invite a rating (iOS `ReviewHistory`, `ReviewCompletion`, `ReviewInvitation`): after the person has used the
// app for a while and just read a finished answer to their own message. Device-local; requests are attempts, never
// evidence of a rating. Times are epoch milliseconds.

private const val DAY = 86_400_000L

/** The device-local policy's memory, saved as JSON. */
@Serializable data class ReviewHistory(
    val firstUse: Long,
    val activeDays: Set<String> = emptySet(),
    val completed: Int = 0,
    val requests: List<Request> = emptyList(),
    val requestedVersions: Set<String> = emptySet(),
    /** Messages this phone sent (by client nonce) and when, waiting for their answer. */
    val sent: Map<String, Long> = emptyMap(),
) {
    @Serializable data class Request(val date: Long, val version: String, val completed: Int)

    fun recordingActivity(now: Long, zone: ZoneId = ZoneId.systemDefault()): ReviewHistory {
        val day = Instant.ofEpochMilli(now).atZone(zone).toLocalDate()
        val days = activeDays + "${day.year}-${day.monthValue}-${day.dayOfMonth}"
        // Only the three-day milestone matters; keep a bounded set after reaching it.
        return copy(activeDays = if (days.size > 3) days.sorted().takeLast(3).toSet() else days)
    }

    /** A week since first use, three active days, five finished answers, once per version, twice a year, 120 days apart. */
    fun eligible(now: Long, version: String): Boolean {
        if (now - firstUse < 7 * DAY || activeDays.size < 3 || completed < 5 || version in requestedVersions ||
            requests.count { now - it.date < 365 * DAY } >= 2) return false
        val last = requests.lastOrNull() ?: return true
        return now - last.date >= 120 * DAY && completed - last.completed >= 5
    }

    fun recordingRequest(now: Long, version: String): ReviewHistory = copy(requestedVersions = requestedVersions + version,
        requests = requests.filter { now - it.date < 365 * DAY } + Request(now, version, completed))

    fun pruned(now: Long): ReviewHistory = copy(sent = sent.filterValues { now - it < 30 * DAY })
}

/** Which of this phone's messages a just-read final answer finished; only the host's turn and final flags count, never content. */
object ReviewCompletion {
    fun match(entries: List<Entry>, sent: Map<String, Long>, now: Long): List<String>? {
        val chat = entries.filter { it.isChat && "call" !in it.data }
        val final = chat.lastOrNull() ?: return null
        if (final.kind != "agent" || final.data["final"]?.jsonPrimitive?.booleanOrNull != true || final.turn <= 0) return null
        val turn = entries.filter { it.turn == final.turn }
        // Non-successful completions and stops produce notices; be conservative about all notices.
        if (turn.any { it.kind == "notice" || it.status == "failed" || it.status == "cancelled" }) return null
        val nonces = turn.mapNotNull { entry ->
            val nonce = entry.clientNonce ?: return@mapNotNull null
            val at = sent[nonce] ?: return@mapNotNull null
            nonce.takeIf { entry.kind == "user" && entry.status == "sent" && "call" !in entry.data && "routineId" !in entry.data &&
                "from" !in entry.data && now - at < 30 * DAY }
        }
        return nonces.ifEmpty { null }
    }
}

/** Why an opportunity to invite a rating went away. */
enum class ReviewCancellation(val wire: String) { Background("background"), Interaction("interaction"), Unavailable("unavailable") }

/**
 * The invitation's state (iOS `ReviewInvitation`): a send, a finished answer read in its chat, then returning to the
 * roster makes an opportunity; the roster asks the store once it has stayed clear. [save] keeps the history, [events]
 * reports what happened (the analytics events).
 */
class ReviewInvitation(initial: ReviewHistory, private val save: (ReviewHistory) -> Unit, private val events: (Event) -> Unit) {
    sealed interface Event {
        data class Opportunity(val completed: Int) : Event
        data class Cancelled(val reason: ReviewCancellation) : Event
        data class Requested(val completed: Int) : Event
    }

    var history: ReviewHistory = initial; private set
    private var activeSince: Long? = null
    private var viewedChat: String? = null
    /** The current opportunity's token; the roster waits on it. */
    var opportunity: Long? = null; private set
    private var tokens = 0L
    private val interruptions = mutableSetOf<Any>()
    val interrupted: Boolean get() = interruptions.isNotEmpty()

    fun interrupt(token: Any, active: Boolean) {
        if (active) { interruptions += token; cancel(ReviewCancellation.Unavailable) } else interruptions -= token
    }

    fun setActive(active: Boolean, now: Long) {
        if (active) { history = history.pruned(now); save(history); if (activeSince == null) activeSince = now }
        else { activeSince = null; viewedChat = null; cancel(ReviewCancellation.Background) }
    }

    fun sent(nonce: String, now: Long) {
        var next = history.pruned(now)
        if (next.sent.size >= 100) next.sent.minByOrNull { it.value }?.let { oldest -> next = next.copy(sent = next.sent - oldest.key) }
        history = next.copy(sent = next.sent + (nonce to now)).recordingActivity(now)
        viewedChat = null
        cancel(ReviewCancellation.Interaction)
        save(history)
    }

    /** An unobscured final answer at the bottom of [chat] stayed in sight. */
    fun viewed(chat: String, entries: List<Entry>, now: Long) {
        if (activeSince == null || interrupted) return
        val done = ReviewCompletion.match(entries, history.sent, now) ?: return
        history = history.copy(sent = history.sent - done.toSet(), completed = history.completed + 1).recordingActivity(now)
        viewedChat = chat
        save(history)
    }

    fun returned(from: String, version: String, now: Long) {
        val since = activeSince
        val viewed = viewedChat
        viewedChat = null
        if (interrupted || viewed != from || since == null || now - since < 60_000 || !history.eligible(now, version)) return
        opportunity = ++tokens
        events(Event.Opportunity(history.completed))
    }

    fun cancel(reason: ReviewCancellation) {
        if (opportunity == null) return
        opportunity = null
        events(Event.Cancelled(reason))
    }

    /** Saved before the store is asked: a request it doesn't show still uses up this app's own cooldown. */
    fun request(token: Long, version: String, now: Long): Boolean {
        val since = activeSince ?: return false
        if (interrupted || opportunity != token || now - since < 60_000 || !history.eligible(now, version)) return false
        opportunity = null
        history = history.recordingRequest(now, version)
        save(history)
        events(Event.Requested(history.completed))
        return true
    }
}
