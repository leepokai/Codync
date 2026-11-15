package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.flow.update

// A task's own transcript, read from its item's details. Its entries never enter a chat (`AppState.entries`) nor the
// mirror: they are held per task id while the transcript is open.

/**
 * A task's transcript opened: as the computer has it (newest 500), kept current from `taskEntry` events until
 * [closeTask].
 */
internal suspend fun AppStore.loadTask(botId: String, taskId: String) {
    // Held before the computer answers, so entries arriving meanwhile are kept and merged.
    mutableState.update { it.copy(taskEntries = it.taskEntries + (taskId to it.taskEntries[taskId].orEmpty())) }
    val entries = entriesOf(hostCall("taskEntries", body("botId" to botId, "taskId" to taskId), 20_000))
    mutableState.update { state ->
        val held = state.taskEntries[taskId] ?: return@update state
        state.copy(taskEntries = state.taskEntries + (taskId to mergeTaskEntries(held, entries)))
    }
}

/** The task's transcript closed: it is let go. */
internal fun AppStore.closeTask(taskId: String) = mutableState.update { it.copy(taskEntries = it.taskEntries - taskId) }

internal fun AppStore.receiveTasks(events: List<MirrorEvent>) = mutableState.update { state ->
    val received = state.taskEntries.receivingTasks(events)
    if (received === state.taskEntries) state else state.copy(taskEntries = received)
}
