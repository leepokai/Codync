package com.codync.android

/**
 * Chats kept deep in memory. The open chat is fully resident, plus any chat a thread is open for and the few most
 * recently opened when a capacity is given. Every other chat holds only a short preview. Up to [REMEMBERED] ids stay
 * ordered for that preview. Nothing here is saved.
 */
class ResidentBots(private val capacity: Int = 0) {
    init { require(capacity >= 0) }

    private val gate = Any()
    /** Oldest first; never longer than [capacity]. */
    private val recent = ArrayDeque<String>()
    /** Oldest first; preview order only, never longer than [REMEMBERED]. */
    private val remembered = ArrayDeque<String>()
    /** Chats a thread's own chat shows now, each with the open threads holding it. Released when its last thread closes. */
    private val held = LinkedHashMap<String, MutableSet<String>>()

    /** Remembers [botId] as just opened. Blank ids are ignored. */
    fun touch(botId: String) {
        if (botId.isBlank()) return
        synchronized(gate) {
            recent.remove(botId)
            recent.addLast(botId)
            while (recent.size > capacity) recent.removeFirst()
            remember(botId)
        }
    }

    /**
     * Keeps [botId] resident while its thread [ticket] is open, outside the recently-opened capacity. One chat can be
     * held by more than one thread at once; [release] needs the same ticket. Blank ids are ignored.
     */
    fun hold(botId: String, ticket: String = botId) {
        if (botId.isBlank()) return
        synchronized(gate) {
            held.getOrPut(botId) { LinkedHashSet() } += ticket
            remember(botId)
        }
    }

    /** The thread [ticket] showing [botId] closed; the chat stays held while another thread still shows it. */
    fun release(botId: String, ticket: String = botId) {
        if (botId.isBlank()) return
        synchronized(gate) {
            val tickets = held[botId] ?: return@synchronized
            tickets -= ticket
            if (tickets.isEmpty()) held -= botId
        }
    }

    private fun remember(botId: String) {
        remembered.remove(botId)
        remembered.addLast(botId)
        while (remembered.size > REMEMBERED) remembered.removeFirst()
    }

    /** The recently opened chats, the ones held by a thread, plus the [open] chat, which is always resident. */
    fun full(open: String?): Set<String> = synchronized(gate) {
        held.keys + recent.toSet() + listOfNotNull(open?.takeIf { it.isNotBlank() })
    }

    /** Chats [touch] has seen, newest first. This does not make them fully resident. */
    fun previewPriority(): List<String> = synchronized(gate) { remembered.reversed() }

    private companion object {
        const val REMEMBERED = 8
    }
}

/**
 * How many views each `(bot, thread)` has. Its owner is held while the first view opens and only released when its last
 * thread's last view closes, so closing one of two open threads cannot make the other thread's card unreachable.
 */
internal class ThreadViews(private val residents: ResidentBots) {
    private data class Key(val botId: String, val threadId: String)

    private val views = HashMap<Key, Int>()

    /** A view of [threadId] opened. True when this is the first view, which should publish the held owner. */
    @Synchronized fun watch(botId: String, threadId: String): Boolean {
        if (botId.isBlank() || threadId.isBlank()) return false
        val key = Key(botId, threadId)
        val shown = (views[key] ?: 0) + 1
        views[key] = shown
        if (shown != 1) return false
        residents.hold(botId, threadId)
        return true
    }

    /** A view of [threadId] closed. True when its last view closed and its transcript should be let go. */
    @Synchronized fun close(botId: String, threadId: String): Boolean {
        if (botId.isBlank() || threadId.isBlank()) return false
        val key = Key(botId, threadId)
        val shown = views[key] ?: return false
        if (shown > 1) {
            views[key] = shown - 1
            return false
        }
        views -= key
        residents.release(botId, threadId)
        return true
    }
}
