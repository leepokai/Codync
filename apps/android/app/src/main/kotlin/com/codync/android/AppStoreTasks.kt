package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject

// Background tasks: a task's own transcript, read from its details, and stopping one. Their entries never enter a chat
// (`AppState.entries`) nor the mirror: they are held per task id while its details are open.

/**
 * A task's details opened: its transcript as the computer has it (newest 500), kept current from `taskEntry` events
 * until [closeTask]. Answers the task itself, which may have ended by now.
 */
internal suspend fun AppStore.loadTask(botId: String, taskId: String): BotTask {
    // Held before the computer answers, so entries arriving meanwhile are kept and merged.
    mutableState.update { it.copy(taskEntries = it.taskEntries + (taskId to it.taskEntries[taskId].orEmpty())) }
    val response = hostCall("taskEntries", body("botId" to botId, "taskId" to taskId), 20_000)
    val entries = entriesOf(response)
    mutableState.update { state ->
        val held = state.taskEntries[taskId] ?: return@update state
        state.copy(taskEntries = state.taskEntries + (taskId to mergeTaskEntries(held, entries)))
    }
    return WireJson.decodeFromJsonElement<BotTask>(response.jsonObject.getValue("task"))
}

/** The task's details closed: its transcript is let go. */
internal fun AppStore.closeTask(taskId: String) = mutableState.update { it.copy(taskEntries = it.taskEntries - taskId) }

internal fun AppStore.receiveTasks(events: List<MirrorEvent>) = mutableState.update { state ->
    val received = state.taskEntries.receivingTasks(events)
    if (received === state.taskEntries) state else state.copy(taskEntries = received)
}

/** Stops one of [botId]'s background tasks, or all of them with no [taskId]. */
internal fun AppStore.stopTask(botId: String, taskId: String?) =
    command("stopTask", body("botId" to botId, "taskId" to taskId), stopTaskKey(botId))

internal fun stopTaskKey(botId: String) = "stopTask:$botId"
