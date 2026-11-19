package com.codync.android.core

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
//
// Such a host says `item` for a task and `task` for a thread. This file is the only place that knows those names:
// what goes out is renamed for it ([hostCallFor]), and what comes in is read in either shape with no version check
// ([oldWorkRow], [threadOfEntryEvent], [threadNamed]; `Bot`'s `@JsonNames`), so the store, the models and the
// screens know only task and thread.

/** The first host version that says task and thread on the wire. */
const val THREADS_HOST = "2.51.0"

/**
 * The host at [hostVersion] takes the new names. One whose version isn't known (no `hello` yet) or can't be read
 * counts as current, like every unreadable version ([AppVersion.isBelow]): it gets the new names.
 */
fun speaksThreads(hostVersion: String?): Boolean = hostVersion == null || !AppVersion.isBelow(hostVersion, THREADS_HOST)

/** The calls that name a task by `taskId` (`itemId` before). */
private val TASK_CALLS = setOf("send", "steer", "unqueue", "workFirst", "workCutIn", "dropWork", "stopWork", "retryWork")

/** [method] and [body] as the host at [hostVersion] takes them: unchanged for one that speaks threads. */
fun hostCallFor(hostVersion: String?, method: String, body: JsonObject): Pair<String, JsonObject> = when {
    speaksThreads(hostVersion) -> method to body
    method == "threadEntries" -> "taskEntries" to body.renamed("threadId" to "taskId")
    method in TASK_CALLS -> method to body.renamed("taskId" to "itemId")
    method == "updateBot" || method == "createBot" -> method to body.renamed("maxThreads" to "maxAgents", "threadWorktrees" to "taskWorktrees")
    else -> method to body
}

private fun JsonObject.renamed(vararg names: Pair<String, String>): JsonObject {
    val old = names.toMap()
    return JsonObject(entries.associate { (key, value) -> (old[key] ?: key) to value })
}

/** A row of `bot.work` with its thread under `thread`: an old host sends it as `task`. */
internal fun Map<String, JsonElement>.oldWorkRow(): Map<String, JsonElement> =
    if ("thread" in this || "task" !in this) this else this - "task" + ("thread" to getValue("task"))

/** The thread of a `threadEntry` event, or of an old host's `taskEntry` (`task`). */
internal fun JsonObject.threadOfEntryEvent(): String? = text("thread") ?: text("task")

/**
 * An entry's `data` in the new names. From an old host `task` is an object `{id, title}`, which is the thread, and
 * `item` the task's id; from a current one `task` is that id (a string) and the data stays as it is.
 */
fun JsonObject.threadNamed(): JsonObject {
    val thread = this["task"] as? JsonObject
    if (thread == null && "item" !in this) return this
    val rest = if (thread == null) this - "item" else this - "item" - "task" + ("thread" to thread)
    return JsonObject(rest + listOfNotNull(this["item"]?.let { "task" to it }))
}

/** This entry with its `data` in the new names ([threadNamed]). */
fun Entry.threadNamed(): Entry = data.threadNamed().let { if (it === data) this else copy(data = it) }
