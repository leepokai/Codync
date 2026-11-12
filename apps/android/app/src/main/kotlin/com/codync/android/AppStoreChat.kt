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
    if (id != null) residents.touch(id)
    // The chat left gives its extra rows back, so it pages its earlier ones again if opened later.
    if (leaving) state.value.selectedBot?.let { left ->
        windows.remove(left)
        mutableState.update { it.copy(historyEnd = it.historyEnd - left) }
    }
    mutableState.update { it.copy(selectedBot = id, trace = false, callRequest = it.callRequest.takeUnless { leaving }) }
    if (id != null) { opened(this); acknowledgeVisible() }
    // The open chat swaps its preview for its window.
    if (leaving) contextScope.launch { refresh() }
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

/** Pages the open chat's earlier rows. [failed] hears when that page didn't load (its error shows as usual). */
fun AppStore.loadHistory(failed: () -> Unit = {}) {
    val id = state.value.selectedBot ?: return
    val started = action(historyKey(id)) {
        try { pageHistory(id) }
        catch (error: CancellationException) { throw error }
        catch (error: Exception) { failed(); throw error }
    }
    if (!started) failed()
}

private suspend fun AppStore.pageHistory(id: String) {
    // beforeSeq is the open window's oldest row, not the preview from before this chat was resident. When that
    // read already reached further back, stop; the screen asks again only while its earlier row is still showing.
    val last = published
    if (last == null || id !in last.full || last.limit(id) != residency().limit(id)) {
        val loadedFrom = floors[id]
        refresh()
        if (floors[id] != loadedFrom) return
    }
    // The oldest row SQLite handed the window, counting rows a queued removal hides from the screen.
    val before = floors[id] ?: Long.MAX_VALUE
    // Older rows the phone already stored come from SQLite before the computer is asked for a page of them.
    if (!withContext(Dispatchers.IO) { requireNotNull(database).hasOlder(id, before) } && !fetchHistoryPage(id, before)) return
    // The older rows are on the phone now. The window grows over them whatever they hold, so a page of only tool or
    // thought rows moves the chat too.
    if (state.value.selectedBot == id) windows[id] = residency().widened(id)
    refresh()
    // The window grew and the same rows came back: this chat's memory budget is spent.
    if (floors[id] == before && state.value.selectedBot == id) mutableState.update { it.copy(historyEnd = it.historyEnd + (id to HistoryEnd.MemoryFull)) }
}

/** Stores the computer's page of [id]'s rows older than [before]. Whether it held any. */
private suspend fun AppStore.fetchHistoryPage(id: String, before: Long): Boolean {
    val mirror = requireNotNull(database)
    if (withContext(Dispatchers.IO) { mirror.historyComplete(id) }) {
        mutableState.update { it.copy(historyEnd = it.historyEnd + (id to HistoryEnd.Complete)) }
        return false
    }
    val response = ready().call("history", buildJsonObject { put("botId", id); put("beforeSeq", before); put("limit", 100) })
    val entries = response.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
    withContext(Dispatchers.IO) {
        mirror.receive(entries)
        if (entries.size < 100) mirror.finishHistory(id)
    }
    // The reader may have gone to another chat while the computer answered: its rows are kept, but nothing here is theirs now.
    if (entries.size < 100 && state.value.selectedBot == id) mutableState.update { it.copy(historyEnd = it.historyEnd + (id to HistoryEnd.Complete)) }
    return entries.isNotEmpty()
}

/** The earlier row pages only once it is actually on screen, and only when that page would move the chat. */
internal fun pageEarlier(
    settled: Boolean, visible: Boolean, loading: Boolean, shown: Int, floor: Long, askedShown: Int, askedFloor: Long,
): Boolean = settled && visible && !loading && (shown > askedShown || floor < askedFloor)

/**
 * When the open chat's earlier-messages row asks for a page, during one visit ([pageEarlier]). A page that failed is
 * asked for again once the row comes back on screen (the iPhone's `onAppear`), never in a loop while the computer is out of reach.
 */
internal class EarlierPaging {
    private var askedShown = -1
    private var askedFloor = Long.MAX_VALUE
    private var lastFailed = false

    /** Whether to page now. Asking remembers [shown] and [floor], so the next page waits until this one moved them. */
    fun ask(settled: Boolean, visible: Boolean, loading: Boolean, shown: Int, floor: Long): Boolean {
        if (!visible) left()
        if (!pageEarlier(settled, visible, loading, shown, floor, askedShown, askedFloor)) return false
        askedShown = shown; askedFloor = floor; lastFailed = false
        return true
    }

    /** The page asked for last didn't load. */
    fun failed() { lastFailed = true }

    /** The row is off screen, or the list dropped it before it could see that: after a failure, it may ask again when it shows anew. */
    fun left() {
        if (lastFailed) { lastFailed = false; askedShown = -1; askedFloor = Long.MAX_VALUE }
    }
}

/** One history load per chat at a time, so a chat still loading doesn't make the one opened next wait. */
internal fun historyKey(botId: String) = "history:$botId"

fun AppStore.retry(send: PendingSend) = action("send:" + send.nonce) { deliver(send.copy(status = SendStatus.Sending, error = null)) }
fun AppStore.discard(send: PendingSend) = action("discard:" + send.nonce) {
    require(send.status == SendStatus.Failed) { "Cancel a queued message before discarding it." }
    withContext(Dispatchers.IO) { requireNotNull(database).discard(send.nonce) }
    refresh()
}
fun AppStore.cancelQueued(send: PendingSend) = action("cancel:" + send.nonce) {
    if (discardsOnPhone(state.value.computer.cloud, send.status)) {
        withContext(Dispatchers.IO) { requireNotNull(database).discard(send.nonce) }
        refresh()
        return@action
    }
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
        // The sent message may sit in older rows this phone holds on disk but not in memory.
        if (withContext(Dispatchers.IO) { requireNotNull(database).hasNonce(send.botId, send.nonce) }) return
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
        savePending(afterFailure(send, state.value.computer.cloud, error))
    } finally {
        sends.remove(send.nonce)
        mutableState.update { it.copy(uploadProgress = it.uploadProgress - send.nonce) }
    }
}

/** A send that failed. With no relay holding it, a message waits here until the computer can be reached (`reconcileMailbox`). */
internal fun afterFailure(send: PendingSend, cloud: String?, error: Exception): PendingSend =
    if (cloud == null && error is IOException && error !is HostError) send.copy(status = SendStatus.Held)
    else send.copy(status = SendStatus.Failed, error = error.message ?: "Couldn't send.")

/** Can't reach a computer with no relay: text waits on this phone until it can (iOS `BotStore.holdsOnPhone`). */
internal fun holdsOnPhone(cloud: String?, link: LinkState): Boolean = cloud == null && when (link) {
    LinkState.Connecting -> true
    is LinkState.Failed -> link.status !in listOf(403, 426)
    else -> false
}

/** No relay holds it, so taking it back touches only this phone (iOS `BotStore.cancelQueued`). */
internal fun discardsOnPhone(cloud: String?, status: SendStatus): Boolean = cloud == null || status == SendStatus.Held

internal suspend fun AppStore.savePending(send: PendingSend) {
    withContext(Dispatchers.IO) { requireNotNull(database).save(send) }
    refresh()
}
fun AppStore.respond(entry: Entry, option: String?) = action("permission:" + entry.id) {
    val active = ready()
    active.call("respondPermission", body("entryId" to entry.id, "optionId" to option))
    delay(2_000)
}

fun AppStore.decide(entry: Entry, option: String) = action("decision:" + entry.id) {
    ready().call("respondDecision", body("entryId" to entry.id, "option" to option))
    delay(2_000)
}

/** A queued message goes now: into the running turn when the agent can take it, else as the next turn (the host decides). */
internal fun AppStore.steer(entry: Entry) = command("steer", body("botId" to entry.botId, "entryId" to entry.id), "steer:" + entry.id)

/** A queued message never goes out: gone at once, back if the computer refuses (its error shows). */
internal fun AppStore.unqueue(entry: Entry) {
    pendingRemovals[entry.id] = entry.rev
    mutableState.update { it.copy(entries = withRemovals(it.entries, pendingRemovals)) }
    command("unqueue", body("botId" to entry.botId, "entryId" to entry.id), "unqueue:" + entry.id, onFailure = {
        pendingRemovals.remove(entry.id)
        contextScope.launch { refresh() }
    })
}

/** [entries] as the screens read them, without the messages taken back: for good once the computer says so (the mirror keeps
 *  its copy, for its rev guard), and those [pending] (with the `rev` each had) until its newer copy says what became of them
 *  (removed, or sent because the agent had taken it first). */
internal fun withRemovals(entries: List<Entry>, pending: Map<String, Long>): List<Entry> =
    entries.filterNot { it.isRemoved || pending[it.id] == it.rev }

/** Carries on with the bot's current conversation instead of the handoff. */
internal fun AppStore.keepConversation(entry: Entry) = command("keepConversation", body("entryId" to entry.id), "handoff:" + entry.id)

/** Saves the user's version of a handoff that hasn't been sent; whether it did (its error shows). */
internal suspend fun AppStore.editHandoff(entry: Entry, text: String): Boolean = try {
    hostCall("editHandoff", body("entryId" to entry.id, "text" to text), 30_000)
    true
} catch (cancelled: CancellationException) { throw cancelled }
catch (error: Exception) { showError(error); false }
