package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.PendingSend
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class QueuedMessagesTest {
    private fun user(id: String, seq: Long, status: String = "sent", nonce: String? = null, removed: Boolean = false) =
        Entry(id, seq, "bot", seq, "user", 1, buildJsonObject {
            put("text", id); put("status", status)
            if (nonce != null) put("clientNonce", nonce)
            if (removed) put("removed", true)
        }, createdAt = seq)

    private fun ids(items: List<ChatItem>) = items.filterIsInstance<ChatItem.Message>().map { it.entry.id }

    @Test fun `while the bot works its queued messages wait above the composer, oldest first`() {
        val chat = listOf(user("later", 3, "queued"), user("sent", 1), user("first", 2, "queued"), user("taken back", 4, "cancelled", removed = true))
        assertEquals(listOf("first", "later"), queuedMessages(chat, working = true).map { it.id })
        // A bot that isn't working starts them at once: they stay in the chat meanwhile.
        assertEquals(emptyList<Entry>(), queuedMessages(chat, working = false))
    }

    @Test fun `the chat leaves out what waits above the composer and what was taken back`() {
        val entries = listOf(user("sent", 1), user("waiting", 2, "queued"), user("taken back", 3, "cancelled", removed = true)).filter { it.isChat }
        assertEquals(listOf("sent"), ids(chatItems(entries, emptyList(), busy = true)))
        assertEquals(listOf("sent", "waiting"), ids(chatItems(entries, emptyList(), busy = false)))
    }

    @Test fun `a message taken back shows removed until the computer's newer copy says what became of it`() {
        val waiting = user("waiting", 2, "queued")
        val removed = withRemovals(listOf(waiting), mapOf("waiting" to waiting.rev)).single()
        assertEquals(true, removed.isRemoved)
        // The agent had already taken it: the host's copy, not removed, brings it back.
        val sent = waiting.copy(rev = waiting.rev + 1, data = user("waiting", 2).data)
        assertEquals(listOf(sent), withRemovals(listOf(sent), mapOf("waiting" to waiting.rev)))
    }

    @Test fun `a send that waits on the computer doesn't show again as this phone's`() {
        val entries = listOf(user("waiting", 2, "queued", nonce = "n"))
        val pending = listOf(PendingSend("n", "bot", "waiting", 1))
        assertEquals(emptyList<ChatItem>(), chatItems(entries, pending, busy = true).filterNot { it is ChatItem.Separator })
    }
}
