package com.codync.android.core

// A task is the agent session working one of a bot's items (`Work.kt`); its transcript is read from the item's details.

/**
 * The task transcripts being read, by task id, with [events]' entries laid over. Only a task whose details are open
 * has a key here: opening them fetches the transcript, so one nobody reads keeps nothing.
 */
fun Map<String, List<Entry>>.receivingTasks(events: List<MirrorEvent>): Map<String, List<Entry>> {
    val incoming = events.filterIsInstance<MirrorEvent.TaskEntryChanged>().filter { it.task in this }.groupBy({ it.task }, { it.entry })
    return if (incoming.isEmpty()) this else this + incoming.mapValues { (task, entries) -> mergeTaskEntries(getValue(task), entries) }
}

/** [held] with [incoming] laid over, oldest first: each entry once, the higher `rev` winning. */
fun mergeTaskEntries(held: List<Entry>, incoming: List<Entry>): List<Entry> =
    (held + incoming).groupBy { it.id }.map { (_, copies) -> copies.maxBy { it.rev } }.sortedWith(compareBy({ it.seq }, { it.id }))
