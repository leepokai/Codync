package com.codync.android

import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.jsonObject

// A thread's own transcript, read and written to from its page ([WorkThreadView]). Its entries never enter a chat
// (`AppState.entries`) nor the mirror: they are held per thread id while the thread is open.

/**
 * A thread's transcript opened: as the computer has it (newest 500), kept current from `threadEntry` events until
 * [closeThread].
 */
internal suspend fun AppStore.loadThread(botId: String, threadId: String) {
    // Held before the computer answers, so entries arriving meanwhile are kept and merged.
    mutableState.update { it.copy(threadEntries = it.threadEntries + (threadId to it.threadEntries[threadId].orEmpty())) }
    val entries = entriesOf(hostCall("threadEntries", body("botId" to botId, "threadId" to threadId), 20_000))
    mutableState.update { state ->
        val held = state.threadEntries[threadId] ?: return@update state
        state.copy(threadEntries = state.threadEntries + (threadId to mergeThreadEntries(held, entries)))
    }
}

/** The thread's transcript closed: it is let go. */
internal fun AppStore.closeThread(threadId: String) = mutableState.update { it.copy(threadEntries = it.threadEntries - threadId) }

internal fun AppStore.receiveThreads(events: List<MirrorEvent>) = mutableState.update { state ->
    val received = state.threadEntries.receivingThreads(events)
    if (received === state.threadEntries) state else state.copy(threadEntries = received)
}

/**
 * The user writes [text] to the thread of [botId]'s task [taskId], with the files of its box's draft [draftKey], which
 * leaves the draft. It shows at once (`AppState.threadSends`), and one the computer refuses (a task that takes no
 * message, one that ended too long ago) stays with why, to send again or drop.
 */
internal fun AppStore.sendToThread(botId: String, taskId: String, draftKey: String, text: String) {
    val files = state.value.drafts[draftKey]?.files.orEmpty()
    if (text.isBlank() && files.isEmpty()) return
    val send = PendingSend(UUID.randomUUID().toString(), botId, text.trim(), System.currentTimeMillis(), files = files)
    lastSentNonce = send.nonce
    mutableState.update { it.copy(drafts = it.drafts - draftKey) }
    persistDrafts()
    deliverToThread(taskId, send)
}

/** Sends [send] to [taskId]'s thread, again too: the same nonce, so the computer never takes it twice. */
internal fun AppStore.deliverToThread(taskId: String, send: PendingSend) {
    mutableState.update { it.holdingThreadSend(taskId, send.copy(status = SendStatus.Sending, error = null)) }
    contextScope.launch {
        try {
            val connection = ready()
            val ids = uploadFiles(connection, send, taskId)
            // Hosts before 2.51.0 ([hostCallFor]): goes when MIN_HOST reaches 2.51.0.
            val (method, body) = hostCallFor(state.value.hello?.text("version"), "send", send.body(ids, taskId))
            val reply = connection.call(method, body, 30_000)
            val entry = Entry.decode(reply.jsonObject.getValue("entry").jsonObject)
            mutableState.update { it.withoutThreadSend(taskId, send.nonce).receivingThreadEntry(entry) }
            withContext(Dispatchers.IO) { send.files.forEach(attachments::discard) }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
            mutableState.update { it.holdingThreadSend(taskId, send.copy(status = SendStatus.Failed, error = error.message ?: "Couldn't send.")) }
        }
    }
}

/** A message to [taskId]'s thread that didn't go is dropped. */
internal fun AppStore.discardThreadSend(taskId: String, nonce: String) {
    val files = state.value.threadSends[taskId].orEmpty().firstOrNull { it.nonce == nonce }?.files.orEmpty()
    mutableState.update { it.withoutThreadSend(taskId, nonce) }
    if (files.isNotEmpty()) contextScope.launch(Dispatchers.IO) { files.forEach(attachments::discard) }
}

/** A message waiting in [taskId]'s thread goes now, as a chat's does ([steer]); the caller names the bot's task, never its thread. */
internal fun AppStore.steerThread(botId: String, taskId: String, entry: Entry) =
    command("steer", body("botId" to botId, "taskId" to taskId, "entryId" to entry.id), "steer:" + entry.id)

/** A message waiting in [taskId]'s thread never goes out: its row leaves when the computer says so (`removed`). */
internal fun AppStore.unqueueThread(botId: String, taskId: String, entry: Entry) =
    command("unqueue", body("botId" to botId, "taskId" to taskId, "entryId" to entry.id), "steer:" + entry.id)

/** [send] among [taskId]'s messages on their way, in the place it had. */
internal fun AppState.holdingThreadSend(taskId: String, send: PendingSend): AppState {
    val held = threadSends[taskId].orEmpty()
    val next = if (held.any { it.nonce == send.nonce }) held.map { if (it.nonce == send.nonce) send else it } else held + send
    return copy(threadSends = threadSends + (taskId to next))
}

internal fun AppState.withoutThreadSend(taskId: String, nonce: String): AppState {
    val rest = threadSends[taskId].orEmpty().filterNot { it.nonce == nonce }
    return copy(threadSends = if (rest.isEmpty()) threadSends - taskId else threadSends + (taskId to rest))
}

/** A message to a thread as `send` answered it, in that thread's transcript (the entry's `botId`) when that is open: there before its event. */
internal fun AppState.receivingThreadEntry(entry: Entry): AppState {
    val held = threadEntries[entry.botId] ?: return this
    return copy(threadEntries = threadEntries + (entry.botId to mergeThreadEntries(held, listOf(entry))))
}
