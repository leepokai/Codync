package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.PendingSend
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class ThreadChatItemsTest {
    private fun entry(id: String, at: Long, kind: String, botId: String = "thread-1", data: JsonObject = JsonObject(emptyMap())) =
        Entry(id, at, botId, 1, kind, createdAt = at, data = data)

    private fun rows(items: List<ChatItem>) = items.mapNotNull {
        when (it) {
            is ChatItem.Message -> it.entry.id
            is ChatItem.Pending -> it.send.nonce
            else -> null
        }
    }

    @Test fun `a thread's chat is what it was told, its messages and what it put in the chat, without its steps`() {
        val told = entry("told", 10, "user", data = buildJsonObject { put("text", "Fix login"); put("from", "host") })
        val tool = entry("tool", 20, "tool")
        val thought = entry("thought", 21, "thought", data = buildJsonObject { put("text", "Hmm") })
        val reply = entry("reply", 30, "agent", data = buildJsonObject { put("text", "Done"); put("final", true) })
        val mine = entry("mine", 40, "user", data = buildJsonObject { put("text", "Also the tests"); put("status", "queued"); put("clientNonce", "n1") })
        val card = entry("card", 25, "permission", botId = "b", data = buildJsonObject {
            put("status", "pending"); putJsonObject("thread") { put("id", "thread-1") }
        })
        val chatOnly = entry("chat", 26, "agent", botId = "b", data = buildJsonObject { put("text", "Hi"); put("final", true) })
        val sends = listOf(PendingSend("n1", "b", "Also the tests", 40), PendingSend("n2", "b", "Later", 50))
        val entries = listOf(told, tool, thought, reply, mine)
        // While it works, the user's waiting message sits over the composer; its send already arrived as that message.
        assertEquals(listOf("told", "card", "reply", "n2"), rows(threadChatItems(entries, listOf(card, chatOnly), "thread-1", sends, live = true)))
        assertEquals(listOf("told", "card", "reply", "mine", "n2"), rows(threadChatItems(entries, listOf(card, chatOnly), "thread-1", sends, live = false)))
        // A task no thread has taken has nothing of the chat's.
        assertEquals(listOf("told", "reply", "mine"), rows(threadChatItems(entries, listOf(card), null, emptyList(), live = false)))
    }
}
