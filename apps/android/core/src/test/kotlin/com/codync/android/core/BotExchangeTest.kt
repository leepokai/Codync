package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class BotExchangeTest {
    private fun notice(id: String, seq: Long, chat: String, source: String, target: String, status: String? = null, reply: String? = null, rev: Long = 1) =
        Entry(id, seq, chat, rev = rev, kind = "notice", createdAt = seq * 1000, data = buildJsonObject {
            put("botMessage", buildJsonObject { put("sourceBotId", source); put("targetBotId", target); put("text", "ask $id"); reply?.let { put("reply", it) } })
            status?.let { put("status", it) }
        })

    @Test fun `structured notices become exchanges and plain notices stay plain`() {
        val x = BotExchange.of(notice("a", 1, "miles", "miles", "dex", "completed", "done"))!!
        assertTrue(x.isOutgoing); assertEquals("dex", x.peerId); assertEquals("Messaged", x.verb); assertEquals(BotExchange.Outcome.Done, x.outcome)
        assertEquals("Message from", BotExchange.of(notice("b", 2, "dex", "miles", "dex"))!!.verb)
        assertNull(BotExchange.of(Entry("c", 3, "miles", rev = 1, kind = "notice", data = buildJsonObject { put("text", "Routine ran") })))
    }

    @Test fun `the conversation merges fetched and live copies and keeps one peer`() {
        val old = notice("a", 1, "miles", "miles", "dex", rev = 1)
        val newer = notice("a", 1, "miles", "miles", "dex", "completed", "ok", rev = 2)
        val other = notice("b", 2, "miles", "miles", "ana")
        val conversation = BotExchange.conversation(listOf(old, other), listOf(newer), "dex")
        assertEquals(1, conversation.size); assertEquals(BotExchange.Outcome.Done, conversation.single().outcome)
        val rows = BotConversationRow.build(conversation)
        assertEquals(listOf("sep-a", "a-request", "a-reply"), rows.map { it.id })
        val pending = BotConversationRow.build(BotExchange.conversation(listOf(old), emptyList(), "dex"))
        assertTrue(pending.last() is BotConversationRow.Status)
    }

    @Test fun `a peer on another computer is named by its exchanges`() {
        val remote = Entry("r", 1, "miles", rev = 1, kind = "notice", createdAt = 1000, data = buildJsonObject {
            put("botMessage", buildJsonObject {
                put("sourceBotId", "miles"); put("targetBotId", "reviewer@desk"); put("text", "review"); put("peerName", "Reviewer on desk")
            })
        })
        val group = BotExchangeGroup(listOf(BotExchange.of(remote)!!, BotExchange.of(notice("b", 2, "miles", "miles", "reviewer@desk"))!!))
        assertEquals("reviewer@desk", group.peerId); assertEquals("Reviewer on desk", group.peerName)
    }
}
