package com.codync.android.core

import java.time.Instant
import java.time.ZoneId
import kotlin.math.abs

/** The IANA zones a routine can run in, and the search over them. */
object TimeZones {
    /** Every zone id, sorted. */
    val ids: List<String> by lazy { iana(ZoneId.getAvailableZoneIds()) }

    /**
     * Only ids the host's IANA database knows: Android's list adds Java's three-letter aliases
     * (ACT, AET…) and the retired SystemV zones. EST, MST and HST are IANA zones and stay.
     */
    internal fun iana(ids: Collection<String>): List<String> =
        ids.filter { id -> '/' !in ZoneId.SHORT_IDS[id].orEmpty() && !id.startsWith("SystemV/") }.sorted()

    /** The zone's offset at `now`: "GMT+8", "GMT-3:30", or "GMT"; empty for an unknown zone. */
    fun offset(id: String, now: Instant = Instant.now()): String {
        val seconds = runCatching { ZoneId.of(id).rules.getOffset(now).totalSeconds }.getOrElse { return "" }
        if (seconds == 0) return "GMT"
        val minutes = abs(seconds) / 60
        val rest = if (minutes % 60 == 0) "" else ":" + (minutes % 60).toString().padStart(2, '0')
        return "GMT${if (seconds < 0) "-" else "+"}${minutes / 60}$rest"
    }

    /**
     * The zones matching `query`, best first: the id starts with it, then every word starts a word
     * of the id, then every word is inside it, then its letters appear in order. Case is ignored and
     * "/", "_" and "-" count as spaces, so "new york" and "asia tai" match.
     */
    fun search(query: String, zones: List<String> = ids): List<String> {
        val words = words(query)
        if (words.isEmpty()) return zones
        return zones.mapNotNull { zone -> rank(words(zone), words)?.let { zone to it } }.sortedBy { it.second }.map { it.first }
    }

    private fun words(text: String) = text.lowercase().split('/', '_', '-', ' ').filter(String::isNotEmpty)

    private fun rank(name: List<String>, words: List<String>): Int? {
        val joined = name.joinToString(" ")
        return when {
            joined.startsWith(words.joinToString(" ")) -> 0
            words.all { word -> name.any { it.startsWith(word) } } -> 1
            words.all { it in joined } -> 2
            inOrder(words.joinToString(""), name.joinToString("")) -> 3
            else -> null
        }
    }

    private fun inOrder(letters: String, text: String): Boolean {
        var at = 0
        for (letter in letters) at = text.indexOf(letter, at).takeIf { it >= 0 }?.plus(1) ?: return false
        return true
    }
}
