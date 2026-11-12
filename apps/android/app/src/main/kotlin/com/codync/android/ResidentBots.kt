package com.codync.android

/**
 * Chats kept deep in memory: the open chat and the few opened most recently.
 * Every other chat holds only a short preview, so a large roster is never decoded whole. Nothing here is saved.
 */
class ResidentBots(private val capacity: Int = 5) {
    init { require(capacity >= 0) }

    private val gate = Any()
    /** Oldest first; never longer than [capacity]. */
    private val recent = ArrayDeque<String>()

    /** Remembers [botId] as just opened. Blank ids are ignored. */
    fun touch(botId: String) {
        if (botId.isBlank()) return
        synchronized(gate) {
            recent.remove(botId)
            recent.addLast(botId)
            while (recent.size > capacity) recent.removeFirst()
        }
    }

    /** The recently opened chats plus the [open] one, which is always resident. */
    fun full(open: String?): Set<String> = synchronized(gate) {
        recent.toSet() + listOfNotNull(open?.takeIf { it.isNotBlank() })
    }
}
