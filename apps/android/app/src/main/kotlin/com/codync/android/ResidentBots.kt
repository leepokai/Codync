package com.codync.android

/**
 * Chats kept deep in memory. The open chat is fully resident, plus the few most recently opened when a capacity
 * is given. Every other chat holds only a short preview. Up to [REMEMBERED] ids stay ordered for that preview.
 * Nothing here is saved.
 */
class ResidentBots(private val capacity: Int = 0) {
    init { require(capacity >= 0) }

    private val gate = Any()
    /** Oldest first; never longer than [capacity]. */
    private val recent = ArrayDeque<String>()
    /** Oldest first; preview order only, never longer than [REMEMBERED]. */
    private val remembered = ArrayDeque<String>()

    /** Remembers [botId] as just opened. Blank ids are ignored. */
    fun touch(botId: String) {
        if (botId.isBlank()) return
        synchronized(gate) {
            recent.remove(botId)
            recent.addLast(botId)
            while (recent.size > capacity) recent.removeFirst()
            remembered.remove(botId)
            remembered.addLast(botId)
            while (remembered.size > REMEMBERED) remembered.removeFirst()
        }
    }

    /** The recently opened chats plus the [open] one, which is always resident. Remembered ids are not included. */
    fun full(open: String?): Set<String> = synchronized(gate) {
        recent.toSet() + listOfNotNull(open?.takeIf { it.isNotBlank() })
    }

    /** Chats [touch] has seen, newest first. This does not make them fully resident. */
    fun previewPriority(): List<String> = synchronized(gate) { remembered.reversed() }

    private companion object {
        const val REMEMBERED = 8
    }
}
