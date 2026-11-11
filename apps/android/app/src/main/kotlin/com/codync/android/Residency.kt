package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson

/**
 * How much of each chat [MirrorDatabase.snapshot] loads. A resident chat (open or recently opened) keeps its
 * newest [window] chat rows ([com.codync.android.core.Entry.isChat]), more while the reader pages back
 * (see [windows]) up to [MAX_WINDOW]. Tool and thought rows between those chat rows come along, so they do not
 * use up the quota; the read still stops after a bounded multiple of [window] and never loads a chat whole.
 * Every other chat keeps its newest [preview] rows. Older rows stay in SQLite and are read on demand.
 */
data class Residency(
    val full: Set<String>,
    val preview: Int = 30,
    val window: Int = DEFAULT_WINDOW,
    val windows: Map<String, Int> = emptyMap(),
) {
    fun limit(botId: String): Int = (windows[botId] ?: window).coerceIn(window, maxOf(window, MAX_WINDOW))

    /**
     * [botId]'s window once older rows reached the phone, from SQLite or a page from the computer: a page further back,
     * whatever those rows hold, so a page of only tool or thought rows moves the chat too.
     */
    fun widened(botId: String): Int = limit(botId) + PAGE

    companion object {
        const val DEFAULT_WINDOW = 300
        /** Rows a chat's window grows by each time the reader pages back. */
        const val PAGE = 200
        /** The most rows one chat holds in memory, however far the reader has paged back. */
        const val MAX_WINDOW = 3000

        /** Whether the open chat should ask for older rows. [shown] counts rows the chat list shows, not tool or thought rows. */
        fun needsEarlier(shown: Int, complete: Boolean): Boolean = !complete && shown < MAX_WINDOW
    }
}

/**
 * A chat's rows from its stored [newestFirst] payloads, oldest first: through the [target]-th chat row, until [budget]
 * characters are passed, its newest row always kept. Hidden rows between those chat rows stay, so Trace and the working
 * indicator still see them.
 */
internal fun newestWindow(newestFirst: Sequence<String>, target: Int, budget: Long): List<Entry> {
    val decoded = mutableListOf<Entry>()
    var shown = 0
    var bytes = 0L
    val rows = newestFirst.iterator()
    while (shown < target && bytes < budget && rows.hasNext()) {
        val text = rows.next()
        bytes += text.length
        val entry = WireJson.decodeFromString<Entry>(text)
        decoded += entry
        if (entry.isChat) shown++
    }
    return decoded.asReversed()
}
