package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.isInternal

/** Queued user entries already hold the next turn; they are not the work the indicator unfolds. */
internal fun runningTurn(entries: List<Entry>, working: Boolean): Long? =
    if (working) entries.filter { !it.isInternal && it.status != "queued" }.maxOfOrNull { it.turn } else null
