package com.codync.android.core

// A thread is where one of a bot's tasks is worked, with its own session (`Work.kt`); its transcript is read from the
// task's details.

/**
 * The thread transcripts being read, by thread id, with [events]' entries laid over. Only a thread that is open
 * has a key here: opening them fetches the transcript, so one nobody reads keeps nothing.
 */
fun Map<String, List<Entry>>.receivingThreads(events: List<MirrorEvent>): Map<String, List<Entry>> {
    val incoming = events.filterIsInstance<MirrorEvent.ThreadEntryChanged>().filter { it.thread in this }.groupBy({ it.thread }, { it.entry })
    return if (incoming.isEmpty()) this else this + incoming.mapValues { (thread, entries) -> mergeThreadEntries(getValue(thread), entries) }
}

/** [held] with [incoming] laid over, oldest first: each entry once, the higher `rev` winning. */
fun mergeThreadEntries(held: List<Entry>, incoming: List<Entry>): List<Entry> =
    (held + incoming).groupBy { it.id }.map { (_, copies) -> copies.maxBy { it.rev } }.sortedWith(compareBy({ it.seq }, { it.id }))
