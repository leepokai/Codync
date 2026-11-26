package com.codync.android

import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// A thread's own transcript, read and written to from its chat ([ThreadChat]). Its entries never enter a chat
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

/**
 * One more thread chat shows [threadId] (the same thread can show under its bot's details over it). Its [botId]'s chat
 * stays resident while the thread is open, so a card the thread put there (an approval past the preview) is reachable.
 */
internal fun AppStore.watchThread(botId: String, threadId: String) {
    if (threadViews.watch(botId, threadId)) contextScope.launch { refresh() }
}

/**
 * A thread chat of [threadId] closed. Its owner stays held while another view or another of its threads is open; its
 * transcript is let go only when this thread's last view closes.
 */
internal fun AppStore.closeThread(botId: String, threadId: String) {
    if (!threadViews.close(botId, threadId)) return
    // The last thread and view are gone, and its chat is not the open one: its extra rows go back to the preview with
    // the paging stop that described them. Otherwise reopening the thread would trust a Complete/MemoryFull marker
    // while the older rows it needs are no longer loaded.
    if (botId !in residents.full(state.value.selectedBot)) reclaimChat(botId)
    mutableState.update { it.copy(threadEntries = it.threadEntries - threadId) }
    contextScope.launch { refresh() }
}

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

/**
 * Reads the computer's next page of older threads (`olderThreads`): the newest first, then the ones before the oldest held.
 * Nothing on a host without them, off the link, while a page loads or once none are left. A page that lands after the
 * list was dropped (a new hello, another Delete old threads) is let go.
 */
internal fun AppStore.loadOlderThreads() {
    val now = state.value
    if (threadKeepDays(now.hello) == null || now.actualConnection !is LinkState.Ready) return
    val held = now.olderThreads
    if (held?.more == false) return
    val host = now.hello?.text("hostId")
    action("olderThreads") {
        val reply = hostQuery("olderThreads", buildJsonObject { held?.before?.let { put("before", it) } })
        val page = OlderThreads.decode(reply.jsonObject)
        // Dropped meanwhile (a new hello, another Delete old threads): the page is let go.
        mutableState.update {
            if (it.hello?.text("hostId") != host || (held != null && it.olderThreads == null)) it
            else it.copy(olderThreads = (it.olderThreads ?: OlderThreads(more = true)) + page)
        }
    }
}

/** [bot] with its computer's older threads it doesn't hold among its `work`, as the Threads list and a thread's page show them. */
internal fun AppState.withOlder(bot: Bot): Bot =
    olderThreads?.of(bot.id)?.takeIf { it.isNotEmpty() }?.let { bot.copy(work = mergedWork(bot.work, it)) } ?: bot
