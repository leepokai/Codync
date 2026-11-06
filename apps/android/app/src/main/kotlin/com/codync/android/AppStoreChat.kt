package com.codync.android

import java.io.IOException
import java.util.UUID
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Conversations: opening a bot, history, sending and its retries.

/** Opens a chat on this computer (the account closes any other computer's) or, with null, closes it. */
fun AppStore.openBot(id: String?) {
    if (id == null && state.value.selectedBot == null) return
    // Leaving a chat drops the call "Talk to Codync" asked for there, waiting or not (iOS `cancelVoiceCallRequest`).
    val leaving = id != state.value.selectedBot
    if (leaving) callWait?.cancel()
    mutableState.update { it.copy(selectedBot = id, trace = false, callRequest = it.callRequest.takeUnless { leaving }) }
    if (id != null) { opened(this); acknowledgeVisible() }
}
fun AppStore.showTrace(show: Boolean) { mutableState.update { it.copy(trace = show) } }

/**
 * Starts a call in [botId]'s open chat once this computer is connected (iOS `requestVoiceCall`); the chat takes it
 * (`takeCallRequest`). A computer that can't be reached is an error instead.
 */
fun AppStore.requestCall(botId: String) {
    callWait?.cancel()
    callWait = contextScope.launch {
        val connected = withTimeoutOrNull(20_000) { state.first { it.actualConnection is LinkState.Ready } }
        if (connected == null) showError(IOException("Can't reach the computer. Check that it's on, then try again."))
        else mutableState.update { it.copy(callRequest = botId) }
    }
}

/** Once: a chat that already has a call takes it too, so it can't start one later. */
fun AppStore.takeCallRequest(botId: String): Boolean {
    if (state.value.callRequest != botId) return false
    mutableState.update { it.copy(callRequest = null) }
    return true
}

/** The chat being read (iOS `ReadingConversation`): the open one, still on screen under Full conversation. */
internal val AppState.readingChats: List<String> get() = listOfNotNull(selectedBot)

/** The chats events make unread while they're on screen: a message in one, or its bot counting unread. */
internal fun readAfter(events: List<MirrorEvent>, chats: List<String>): List<String> = chats.filter { chat ->
    events.any { event ->
        when (event) {
            is MirrorEvent.EntryChanged -> event.entry.isChat && event.entry.botId == chat
            is MirrorEvent.BotChanged -> event.bot.id == chat && event.bot.unread > 0
            else -> false
        }
    }
}

/** Marks the chats on screen read (`chats`: only these); the host ignores it when nothing there is unread. */
internal fun AppStore.acknowledgeVisible(chats: List<String> = state.value.readingChats) {
    if (!foreground || channel?.state?.value !is LinkState.Ready) return
    for (chat in chats) action("read:" + chat) { channel?.call("markRead", body("botId" to chat)) }
}

/** The entries of a host reply (`callEntries`, `botConversation`). */
internal fun entriesOf(response: JsonElement): List<Entry> =
    response.jsonObject["entries"]?.jsonArray?.map { Entry.decode(it.jsonObject) }.orEmpty()

/** What a voice call carried (iOS `BotStore.callEntries`): the chat this phone holds may not reach back to it. */
internal suspend fun AppStore.callEntries(botId: String, call: String): List<Entry> =
    entriesOf(hostCall("callEntries", buildJsonObject { put("botId", botId); put("callId", call) }, 20_000))

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
        val result = connection.call("send", send.body(ids))
        val entry = Entry.decode(result.jsonObject.getValue("entry").jsonObject)
        withContext(Dispatchers.IO) { requireNotNull(database).receive(listOf(entry)) }
        refresh()
        val bot = state.value.bots.firstOrNull { it.id == send.botId }
        if (bot != null && PushPreferences.tasks(application) && application.getSystemService(android.app.NotificationManager::class.java).areNotificationsEnabled()) {
            val watch = withContext(Dispatchers.IO) { feed.start(bot) }
            NotificationDisplay.task(application, local.referenceId, id, watch)
            syncPush(); PushDelivery.schedule(application)
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

/** Carries on with the bot's current conversation instead of the handoff. */
internal fun AppStore.keepConversation(entry: Entry) = command("keepConversation", body("entryId" to entry.id), "handoff:" + entry.id)

/** Saves the user's version of a handoff that hasn't been sent; whether it did (its error shows). */
internal suspend fun AppStore.editHandoff(entry: Entry, text: String): Boolean = try {
    hostCall("editHandoff", body("entryId" to entry.id, "text" to text), 30_000)
    true
} catch (cancelled: CancellationException) { throw cancelled }
catch (error: Exception) { showError(error); false }
