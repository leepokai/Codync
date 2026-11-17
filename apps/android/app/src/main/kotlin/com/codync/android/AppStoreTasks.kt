package com.codync.android

import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.jsonObject

// A task's own transcript, read and written to from its item's thread. Its entries never enter a chat
// (`AppState.entries`) nor the mirror: they are held per task id while the thread is open.

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

/**
 * The user writes [text] to the thread of [botId]'s item [itemId]. It shows at once (`AppState.threadSends`), and one
 * the computer refuses (an item that takes no message, one that ended too long ago) stays with why, to send again or drop.
 */
internal fun AppStore.sendToThread(botId: String, itemId: String, text: String) {
    if (text.isBlank()) return
    val send = PendingSend(UUID.randomUUID().toString(), botId, text.trim(), System.currentTimeMillis())
    lastSentNonce = send.nonce
    deliverToThread(itemId, send)
}

/** Sends [send] to [itemId]'s thread, again too: the same nonce, so the computer never takes it twice. */
internal fun AppStore.deliverToThread(itemId: String, send: PendingSend) {
    mutableState.update { it.holdingThreadSend(itemId, send.copy(status = SendStatus.Sending, error = null)) }
    contextScope.launch {
        try {
            val reply = hostCall("send", body("botId" to send.botId, "itemId" to itemId, "text" to send.text, "clientNonce" to send.nonce), 30_000)
            val entry = Entry.decode(reply.jsonObject.getValue("entry").jsonObject)
            mutableState.update { it.withoutThreadSend(itemId, send.nonce).receivingThreadEntry(entry) }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
            mutableState.update { it.holdingThreadSend(itemId, send.copy(status = SendStatus.Failed, error = error.message ?: "Couldn't send.")) }
        }
    }
}

/** A message to [itemId]'s thread that didn't go is dropped. */
internal fun AppStore.discardThreadSend(itemId: String, nonce: String) = mutableState.update { it.withoutThreadSend(itemId, nonce) }

/** A message waiting in [itemId]'s thread goes now, as a chat's does ([steer]); the caller names the bot's item, never its task. */
internal fun AppStore.steerThread(botId: String, itemId: String, entry: Entry) =
    command("steer", body("botId" to botId, "itemId" to itemId, "entryId" to entry.id), "steer:" + entry.id)

/** A message waiting in [itemId]'s thread never goes out: its row leaves when the computer says so (`removed`). */
internal fun AppStore.unqueueThread(botId: String, itemId: String, entry: Entry) =
    command("unqueue", body("botId" to botId, "itemId" to itemId, "entryId" to entry.id), "steer:" + entry.id)

/** [send] among [itemId]'s messages on their way, in the place it had. */
internal fun AppState.holdingThreadSend(itemId: String, send: PendingSend): AppState {
    val held = threadSends[itemId].orEmpty()
    val next = if (held.any { it.nonce == send.nonce }) held.map { if (it.nonce == send.nonce) send else it } else held + send
    return copy(threadSends = threadSends + (itemId to next))
}

internal fun AppState.withoutThreadSend(itemId: String, nonce: String): AppState {
    val rest = threadSends[itemId].orEmpty().filterNot { it.nonce == nonce }
    return copy(threadSends = if (rest.isEmpty()) threadSends - itemId else threadSends + (itemId to rest))
}

/** A thread's message as `send` answered it, in its task's transcript (the entry's `botId`) when that is open: there before its event. */
internal fun AppState.receivingThreadEntry(entry: Entry): AppState {
    val held = taskEntries[entry.botId] ?: return this
    return copy(taskEntries = taskEntries + (entry.botId to mergeTaskEntries(held, listOf(entry))))
}
