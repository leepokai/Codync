package com.codync.android.core

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
//
// Such a host says `item` for a task and `task` for a thread. This file is the only place that knows those names:
// what goes out carries both names ([hostCallFor]), and what comes in is read in either shape with no version check
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

/**
 * [method] and [body] as either host takes them. What names a task or a bot's thread settings carries both names
 * whatever [hostVersion] says, so a version that is missing or left from the last link never sends a thread's
 * message to the chat: each host reads the name it knows and ignores the other. Only `threadEntries` goes by the
 * version: a wrong guess there is a read that fails.
 */
fun hostCallFor(hostVersion: String?, method: String, body: JsonObject): Pair<String, JsonObject> = when {
    method == "threadEntries" ->
        if (speaksThreads(hostVersion)) method to body else "taskEntries" to JsonObject(body - "threadId" + body.oldNames("threadId" to "taskId"))
    method in TASK_CALLS -> method to JsonObject(body + body.oldNames("taskId" to "itemId"))
    method == "updateBot" || method == "createBot" -> method to JsonObject(body + body.oldNames("maxThreads" to "maxAgents", "threadWorktrees" to "taskWorktrees"))
    else -> method to body
}

/** What this holds under each new name, under its old one. */
private fun JsonObject.oldNames(vararg names: Pair<String, String>): Map<String, JsonElement> =
    names.mapNotNull { (new, old) -> this[new]?.let { old to it } }.toMap()

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
