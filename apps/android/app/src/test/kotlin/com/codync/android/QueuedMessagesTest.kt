package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.HostError
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.core.PendingSend
import com.codync.android.core.SendStatus
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class QueuedMessagesTest {
    private fun send(nonce: String, createdAt: Long = 1, status: SendStatus = SendStatus.Sending) =
        PendingSend(nonce, "bot", "hello", createdAt, status)

    @Test fun `with no relay a message that can't go waits on the phone and goes with the same nonce`() {
        val held = afterFailure(send("n1"), null, java.io.IOException("Can't reach the computer."))
        assertEquals(SendStatus.Held, held.status)
        val pending = listOf(send("n2", 2, SendStatus.Held), held, send("n3", 3, SendStatus.Failed))
        assertEquals(listOf("n1", "n2"), readyToSend(pending, null).map { it.nonce })
        assertEquals(setOf(SendStatus.Sending), readyToSend(pending, null).map { it.status }.toSet())
    }

    @Test fun `what waits on the phone still goes after the computer joins an account, and the relay answers for the rest`() {
        val pending = listOf(send("phone", 1, SendStatus.Held), send("relay", 2, SendStatus.Waiting))
        assertEquals(listOf("phone"), readyToSend(pending, "https://api.codync.dev").map { it.nonce })
    }

    @Test fun `with no relay left, what a relay held goes too`() {
        assertEquals(listOf("relay"), readyToSend(listOf(send("relay", 1, SendStatus.Waiting)), null).map { it.nonce })
    }

    @Test fun `a message taken back while an earlier one goes doesn't go`() = runBlocking {
        val pending = mutableListOf("n1", "n2", "n3")
        val delivered = mutableListOf<String>()
        deliverInTurn(pending.map { send(it, status = SendStatus.Held) }, { it.nonce in pending }) { sent ->
            delivered += sent.nonce
            pending -= "n2"
        }
        assertEquals(listOf("n1", "n3"), delivered)
    }

    @Test fun `taking a message back touches only the phone unless a relay holds it`() {
        assertTrue(discardsOnPhone(null, SendStatus.Waiting))
        assertTrue(discardsOnPhone("https://api.codync.dev", SendStatus.Held))
        assertFalse(discardsOnPhone("https://api.codync.dev", SendStatus.Waiting))
    }

    @Test fun `with a relay or a refusal a message that can't go fails`() {
        assertEquals(SendStatus.Failed, afterFailure(send("n1"), "https://api.codync.dev", java.io.IOException("Offline")).status)
        assertEquals(SendStatus.Failed, afterFailure(send("n1"), null, HostError(403, "Turned away")).status)
        assertEquals(SendStatus.Failed, afterFailure(send("n1"), null, IllegalStateException("Bad")).status)
    }

    @Test fun `only a computer with no relay holds text on the phone, unless it turned the phone away`() {
        assertTrue(holdsOnPhone(null, LinkState.Failed("Unreachable")))
        assertTrue(holdsOnPhone(null, LinkState.Connecting))
        assertFalse(holdsOnPhone(null, LinkState.Failed("Revoked", 403)))
        assertFalse(holdsOnPhone(null, LinkState.Ready(HostRoute.Direct)))
        assertFalse(holdsOnPhone("https://api.codync.dev", LinkState.Failed("Unreachable")))
    }

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
