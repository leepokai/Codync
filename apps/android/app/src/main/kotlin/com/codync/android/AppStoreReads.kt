package com.codync.android

import com.codync.android.core.AppVersion
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Read marks: reading the chats on screen, opening one, the roster's "Mark as read" and "Mark as unread". The host decides what a read does to a chat marked unread; here they only go out in order.

/** The first host with `markUnread`. */
private const val MARK_UNREAD_HOST = "2.43.0"

/** The roster offers "Mark as unread" for a chat with nothing unread; otherwise, or on an older host (it refuses `markUnread`), "Mark as read". */
internal fun AppState.offersMarkUnread(bot: Bot): Boolean =
    bot.unread == 0 && !bot.keptUnread && hello?.text("version")?.let { !AppVersion.isBelow(it, MARK_UNREAD_HOST) } == true

/** `markRead`'s body. [automatic]: the chat is on screen, not the user reading it, and the host leaves one marked unread alone. */
internal fun readBody(id: String, automatic: Boolean = false): JsonObject = buildJsonObject {
    put("botId", id)
    if (automatic) put("automatic", true)
}

/** One computer's opens and marks. [run] runs one as the store's action (its error shown, its job awaited on the way out); false when it never started. */
internal class ReadMarks(private val run: (key: String, operation: suspend () -> Unit) -> Boolean) {
    /** Each chat's marks wait in line, retries included, so the host gets them in the order they were made. */
    private val lines = mutableMapOf<String, Mutex>()
    private var made = 0
    /** Chats whose last open never reached the host. */
    private val unsentOpens = mutableSetOf<String>()

    /** The roster's "Mark as read" or "Mark as unread": [send] after the chat's last mark, its failure shown. */
    fun mark(id: String, send: suspend () -> Unit): Boolean {
        unsentOpens.remove(id)
        val line = lines.getOrPut(id) { Mutex() }
        return run("readMark:$id:${made++}") {
            line.withLock {
                // An open before this mark may have failed while it waited: this one is newer.
                unsentOpens.remove(id)
                send()
            }
        }
    }

    /** A message sent from here, which reads the chat on the host: [send] after the chat's last mark, and the next mark after it. */
    suspend fun <T> send(id: String, send: suspend () -> T): T = lines.getOrPut(id) { Mutex() }.withLock { send() }

    /** The user opened the chat, which reads it whatever this phone knows of it. A failure never shows: it goes again the next time the chat is on screen with a link ([retryOpen]). */
    fun open(id: String, send: suspend () -> Unit) {
        val started = mark(id) {
            try { send() }
            catch (error: CancellationException) { throw error }
            catch (_: Exception) { unsentOpens.add(id) }
        }
        if (!started) unsentOpens.add(id)
    }

    /** The chat is on screen with a link: true when that sent an open that hadn't reached the host. */
    fun retryOpen(id: String, send: suspend () -> Unit): Boolean {
        if (id !in unsentOpens) return false
        open(id, send)
        return true
    }
}

private suspend fun AppStore.readAsUser(id: String) {
    ready().call("markRead", readBody(id))
}

/** The roster's "Mark as read". */
fun AppStore.markRead(id: String) { reads.mark(id) { readAsUser(id) } }

/** The roster's "Mark as unread". */
fun AppStore.markUnread(id: String) { reads.mark(id) { receiveReply(ready().call("markUnread", body("botId" to id))) } }

/** The user opened the chat. */
internal fun AppStore.readOpened(id: String) = reads.open(id) { readAsUser(id) }

/** Marks the chats on screen read (`chats`: only these): the host leaves one marked unread alone, and ignores it when nothing there is unread. */
internal fun AppStore.acknowledgeVisible(chats: List<String> = state.value.readingChats) {
    if (!foreground || channel?.state?.value !is LinkState.Ready) return
    for (chat in chats) {
        if (reads.retryOpen(chat) { readAsUser(chat) }) continue
        action("read:$chat") { channel?.call("markRead", readBody(chat, automatic = true)) }
    }
}
