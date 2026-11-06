package com.codync.android

import kotlinx.coroutines.CancellationException

// Read marks: opening a chat, the roster's "Mark as read" and "Mark as unread". The host decides what a read does to a chat marked unread; here they only go out in order.

/** The roster's "Mark as read". */
fun AppStore.markRead(id: String) = sendRead(id) { readOnHost(id) }

/** The user opened the chat, which reads it whatever this phone knows of it. A failure never shows: it's read as opened the next time it's on screen with a link ([acknowledgeVisible]). */
internal fun AppStore.readOpened(id: String) {
    val went = sendRead(id) {
        try { readOnHost(id) }
        catch (error: CancellationException) { throw error }
        catch (_: Exception) { unsentOpens.add(id) }
    }
    if (!went) unsentOpens.add(id)
}

/** Runs [send] after a mark as unread still under way, so the host never gets them the other way round. */
private fun AppStore.sendRead(id: String, send: suspend () -> Unit): Boolean {
    unsentOpens.remove(id)
    val marking = actions["roster:$id"]
    return action("open:$id") {
        marking?.join()
        send()
    }
}

private suspend fun AppStore.readOnHost(id: String) {
    ready().call("markRead", body("botId" to id))
}

/** The roster's "Mark as unread", after a mark as read still under way, so the read can't land last. */
fun AppStore.markUnread(id: String) {
    unsentOpens.remove(id)
    command("markUnread", body("botId" to id), "roster:$id", after = actions["open:$id"])
}
