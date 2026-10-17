package com.codync.android

import android.app.Application
import java.io.IOException
import java.util.UUID
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Conversations: opening a bot or thread, history, sending and its retries.

fun AppStore.openBot(id: String?, thread: String? = null) {
    mutableState.update { it.copy(selectedBot = id, thread = thread, trace = false) }
    if (id != null) {
        acknowledgeVisible()
        if (thread != null) action("thread:" + thread) {
            val active = ready()
            val response = active.call("thread", body("botId" to id, "rootId" to thread))
            receiveEntries(response)
        }
    }
}
fun AppStore.showTrace(show: Boolean) { mutableState.update { it.copy(trace = show) } }

/** A conversation on screen: a bot's main chat (`thread` null) or one of its threads. */
internal data class ReadingScope(val botId: String, val thread: String?)

/**
 * What's being read (iOS `ReadingConversation`): the open chat, still on screen under its thread
 * or Full conversation, and the open thread.
 */
internal val AppState.readingScopes: List<ReadingScope> get() {
    val id = selectedBot ?: return emptyList()
    return listOfNotNull(ReadingScope(id, null), thread?.let { ReadingScope(id, it) })
}

/** The scopes events make unread while they're on screen: a message in one, or its bot counting unread. */
internal fun readAfter(events: List<MirrorEvent>, scopes: List<ReadingScope>): List<ReadingScope> = scopes.filter { scope ->
    events.any { event ->
        when (event) {
            is MirrorEvent.EntryChanged -> event.entry.isChat && event.entry.botId == scope.botId && event.entry.threadId == scope.thread
            is MirrorEvent.BotChanged -> event.bot.id == scope.botId && event.bot.unread > 0
            else -> false
        }
    }
}

/** Marks the conversations on screen read (`scopes`: only these); the host ignores it when nothing there is unread. */
internal fun AppStore.acknowledgeVisible(scopes: List<ReadingScope> = state.value.readingScopes) {
    if (!foreground || channel?.state?.value !is LinkState.Ready) return
    for (scope in scopes) action("read:" + scope.botId + ":" + scope.thread) {
        channel?.call("markRead", body("botId" to scope.botId, "threadId" to scope.thread))
    }
}

fun AppStore.loadHistory() {
    val id = state.value.selectedBot ?: return
    action("history") {
        val mirror = requireNotNull(database)
        if (withContext(Dispatchers.IO) { mirror.historyComplete(id) }) {
            mutableState.update { it.copy(historyComplete = it.historyComplete + id) }
            return@action
        }
        val before = state.value.entries.filter { it.botId == id }.minOfOrNull(Entry::seq) ?: Long.MAX_VALUE
        val active = ready()
        val response = active.call("history", buildJsonObject { put("botId", id); put("beforeSeq", before); put("limit", 100) })
        val entries = response.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
        withContext(Dispatchers.IO) {
            mirror.receive(entries)
            if (entries.size < 100) mirror.finishHistory(id)
        }
        if (entries.size < 100) mutableState.update { it.copy(historyComplete = it.historyComplete + id) }
        refresh()
    }
}

fun AppStore.send(text: String, files: List<OutgoingFile> = emptyList()) {
    val id = state.value.selectedBot ?: return
    if (text.isBlank() && files.isEmpty()) return
    val send = PendingSend(UUID.randomUUID().toString(), id, state.value.thread, text.trim(), System.currentTimeMillis(), files = files)
    action("save:" + send.nonce) { savePending(send); deliver(send) }
}

fun AppStore.retry(send: PendingSend) = action("send:" + send.nonce) { deliver(send.copy(status = SendStatus.Sending, error = null)) }
fun AppStore.discard(send: PendingSend) = action("discard:" + send.nonce) {
    require(send.status == SendStatus.Failed) { "Cancel a queued message before discarding it." }
    withContext(Dispatchers.IO) { requireNotNull(database).discard(send.nonce) }
    refresh()
}
fun AppStore.cancelQueued(send: PendingSend) = action("cancel:" + send.nonce) {
    val active = requireNotNull(channel) { "Reconnect to the relay to cancel this message." }
    when (active.cancelQueued(send.nonce)) {
        "cancelled" -> { withContext(Dispatchers.IO) { requireNotNull(database).discard(send.nonce) }; refresh() }
        "delivering" -> savePending(send.copy(status = SendStatus.Delivering))
        else -> throw IOException("Couldn't take the message back. It may have been delivered already.")
    }
}

internal suspend fun AppStore.deliver(send: PendingSend) {
    if (!sends.add(send.nonce)) return
    try {
        if (state.value.entries.any { it.botId == send.botId && it.clientNonce == send.nonce }) return
        savePending(send)
        val active = channel
        if (active?.state?.value is LinkState.ComputerOffline && send.files.isEmpty()) {
            savePending(send.copy(status = SendStatus.Waiting))
            try { active.enqueue(send); return }
            catch (error: IOException) {
                if (error.message != "hostOnline") throw error
                // Relay presence can lag; use the same nonce after the channel becomes ready.
            }
        }
        val connection = ready()
        val ids = mutableListOf<String>()
        var completed = 0L
        val total = send.files.sumOf { it.size }
        for (file in send.files) {
            val id = UUID.randomUUID().toString()
            HostFiles(connection).upload(send.botId, id, file) { offset ->
                val percent = if (total == 0L) 100 else ((completed + offset) * 100 / total).toInt()
                mutableState.update { it.copy(uploadProgress = it.uploadProgress + (send.nonce to percent)) }
            }
            completed += file.size
            ids.add(id)
            withContext(Dispatchers.IO) { attachments.cache(id, file) }
        }
        val result = connection.call("send", buildJsonObject {
            put("botId", send.botId); put("text", send.text); put("clientNonce", send.nonce)
            send.threadId?.let { put("threadId", it) }
            if (ids.isNotEmpty()) put("attachments", JsonArray(ids.map(::JsonPrimitive)))
        })
        val entry = Entry.decode(result.jsonObject.getValue("entry").jsonObject)
        withContext(Dispatchers.IO) { requireNotNull(database).receive(listOf(entry)) }
        refresh()
        val bot = state.value.bots.firstOrNull { it.id == send.botId }
        if (bot != null && PushPreferences.tasks(getApplication()) && getApplication<Application>().getSystemService(android.app.NotificationManager::class.java).areNotificationsEnabled()) {
            val watch = withContext(Dispatchers.IO) { TaskFeed(local.directory).start(bot) }
            NotificationDisplay.task(getApplication(), local.referenceId, state.value.computers.first().id, watch)
            syncPush(); PushDelivery.schedule(getApplication())
        }
    } catch (error: CancellationException) {
        withContext(NonCancellable) { savePending(send.copy(status = SendStatus.Failed, error = "Sending was interrupted. Retry uses the same message ID.")) }
        throw error
    } catch (error: Exception) {
        savePending(send.copy(status = SendStatus.Failed, error = error.message ?: "Couldn't send."))
    } finally {
        sends.remove(send.nonce)
        mutableState.update { it.copy(uploadProgress = it.uploadProgress - send.nonce) }
    }
}

internal suspend fun AppStore.savePending(send: PendingSend) {
    withContext(Dispatchers.IO) { requireNotNull(database).save(send) }
    refresh()
}
fun AppStore.respond(entry: Entry, option: String?) = action("permission:" + entry.id) {
    val active = ready()
    active.call("respondPermission", body("entryId" to entry.id, "optionId" to option))
    delay(2_000)
}

private suspend fun AppStore.receiveEntries(response: JsonElement) {
    val entries = response.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
    withContext(Dispatchers.IO) { requireNotNull(database).receive(entries) }
    refresh()
}
