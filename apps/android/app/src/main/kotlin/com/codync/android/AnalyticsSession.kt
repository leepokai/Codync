package com.codync.android

import java.util.UUID

/**
 * `$session_id` (iOS `AnalyticsSession`): one id for a stretch of use, so an error's page in PostHog shows the
 * events that led up to it. A new one after 30 minutes without events or 24 hours in all, as PostHog's own SDKs do.
 */
internal class AnalyticsSession {
    private var id: String? = null
    private var started = 0L
    private var last = 0L

    /** The id for an event at [now] (ms); the first event after the last session ran out starts a new one. Any thread. */
    @Synchronized fun id(now: Long): String {
        val current = id
        if (current != null && now - last <= IDLE && now - started <= LONGEST) {
            last = now
            return current
        }
        return uuidV7(now).also { id = it; started = now; last = now }
    }

    companion object {
        private const val IDLE = 30 * 60 * 1000L
        private const val LONGEST = 24 * 60 * 60 * 1000L

        /** PostHog reads a session's start from its UUIDv7: milliseconds since 1970 in the first 48 bits. */
        private fun uuidV7(ms: Long): String {
            val random = UUID.randomUUID()
            val high = (ms shl 16) or 0x7000L or (random.mostSignificantBits and 0x0FFFL)
            val low = (random.leastSignificantBits and 0x3FFF_FFFF_FFFF_FFFFL) or Long.MIN_VALUE
            return UUID(high, low).toString()
        }
    }
}
