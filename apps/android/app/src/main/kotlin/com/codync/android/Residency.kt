package com.codync.android

/**
 * How much of each chat [MirrorDatabase.snapshot] loads. A resident chat (open or recently opened) loads its
 * newest [window] rows, more while the reader pages back (see [windows]) up to [MAX_WINDOW]; every other chat
 * keeps its newest [preview] rows. Older rows stay in SQLite and are read on demand, so no chat is read whole.
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
