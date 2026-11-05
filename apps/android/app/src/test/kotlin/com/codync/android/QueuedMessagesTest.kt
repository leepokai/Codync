package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.PendingSend
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class QueuedMessagesTest {
    private fun user(id: String, seq: Long, status: String = "sent", nonce: String? = null) =
        Entry(id, seq, "bot", seq, "user", 1, buildJsonObject {
            put("text", id); put("status", status)
            if (nonce != null) put("clientNonce", nonce)
        }, createdAt = seq)

    private fun ids(items: List<ChatItem>) = items.filterIsInstance<ChatItem.Message>().map { it.entry.id }

    @Test fun `while the bot works its queued messages wait above the composer, oldest first`() {
        val chat = listOf(user("later", 3, "queued"), user("sent", 1), user("first", 2, "queued"))
        assertEquals(listOf("first", "later"), queuedMessages(chat, working = true).map { it.id })
        // A bot that isn't working starts them at once: they stay in the chat meanwhile.
        assertEquals(emptyList<Entry>(), queuedMessages(chat, working = false))
    }

    @Test fun `the chat leaves out what waits above the composer`() {
        val entries = listOf(user("sent", 1), user("waiting", 2, "queued"))
        assertEquals(listOf("sent"), ids(chatItems(entries, emptyList(), busy = true)))
        assertEquals(listOf("sent", "waiting"), ids(chatItems(entries, emptyList(), busy = false)))
    }

    @Test fun `a message taken back is gone until the computer's newer copy says what became of it`() {
        val waiting = user("waiting", 2, "queued")
        assertEquals(emptyList<Entry>(), withRemovals(listOf(waiting), mapOf("waiting" to waiting.rev)))
        // The agent had already taken it: the host's copy, not removed, brings it back.
        val sent = waiting.copy(rev = waiting.rev + 1, data = user("waiting", 2).data)
        assertEquals(listOf(sent), withRemovals(listOf(sent), mapOf("waiting" to waiting.rev)))
        // The computer took it back too: gone for good.
        val removed = waiting.copy(rev = waiting.rev + 1, data = buildJsonObject { put("status", "cancelled"); put("removed", true) })
        assertEquals(emptyList<Entry>(), withRemovals(listOf(removed), emptyMap()))
    }

    @Test fun `a send that waits on the computer doesn't show again as this phone's`() {
        val entries = listOf(user("waiting", 2, "queued", nonce = "n"))
        val pending = listOf(PendingSend("n", "bot", "waiting", 1))
        assertEquals(emptyList<ChatItem>(), chatItems(entries, pending, busy = true).filterNot { it is ChatItem.Separator })
    }
}
