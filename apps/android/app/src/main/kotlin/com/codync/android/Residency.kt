package com.codync.android

/**
 * How much of each chat [MirrorDatabase.snapshot] loads. A resident chat (open or recently opened) keeps its
 * newest [window] chat rows ([com.codync.android.core.Entry.isChat]), more while the reader pages back
 * (see [windows]) up to [MAX_WINDOW]. Tool and thought rows between those chat rows come along, so they do not
 * use up the quota; the read still stops after a bounded multiple of [window] and never loads a chat whole.
 * Every other chat keeps its newest [preview] rows. Older rows stay in SQLite and are read on demand.
 * A negative [preview] is the load-everything sentinel ([All]), not "no rows".
 */
data class Residency(
    val full: Set<String>,
    val preview: Int = 30,
    val window: Int = DEFAULT_WINDOW,
    val windows: Map<String, Int> = emptyMap(),
    val ceiling: Int = MAX_WINDOW,
) {
    fun limit(botId: String): Int = (windows[botId] ?: window).coerceIn(window, maxOf(window, ceiling))

    /** Whether the open chat should ask for older rows. [shown] counts rows the chat list shows, not tool or thought rows. */
    fun needsEarlier(shown: Int, complete: Boolean): Boolean = !complete && shown < ceiling

    companion object {
        const val DEFAULT_WINDOW = 300
        /** Rows a chat's window grows by each time the reader pages back. */
        const val PAGE = 200
        /** The most rows one chat holds in memory, however far the reader has paged back. */
        const val MAX_WINDOW = 3000

        /** Every entry. Tests and diagnostics only; the app never asks for it. */
        val All = Residency(emptySet(), preview = -1)
    }
}
