package com.codync.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkThreadsTest {
    private fun entry(id: String, seq: Long, rev: Long, text: String = id, botId: String = "thread-1") =
        Entry(id, seq, botId, rev, "agent", data = buildJsonObject { put("text", text) })
    private fun event(thread: String, entry: Entry) = MirrorEvent.ThreadEntryChanged(thread, entry)

    @Test fun `a thread entry event is its own kind, never a chat's entry`() {
        val wire = Json.parseToJsonElement(
            """{"type":"threadEntry","rev":9,"thread":"thread-1","entry":{"id":"e1","seq":3,"botId":"thread-1","rev":9,"kind":"tool","data":{"title":"Run"}}}""").jsonObject
        val event = MirrorEvent.decode(wire)
        assertEquals(MirrorEvent.ThreadEntryChanged("thread-1", Entry("e1", 3, "thread-1", 9, "tool", data = buildJsonObject { put("title", "Run") })), event)
        assertFalse(event is MirrorEvent.EntryChanged)
    }

    @Test fun `a thread entry event this app can't read is passed over`() {
        for (wire in listOf("""{"type":"threadEntry","rev":9,"entry":{"id":"e1","seq":3,"botId":"thread-1","rev":9,"kind":"tool"}}""",
            """{"type":"threadEntry","rev":9,"thread":"thread-1"}""", """{"type":"threadEntry","rev":9,"thread":"thread-1","entry":{"id":""}}""")) {
            assertEquals(wire, MirrorEvent.Ignored, MirrorEvent.decode(Json.parseToJsonElement(wire).jsonObject))
        }
    }

    @Test fun `a thread's entries merge once each, the higher rev winning, oldest first`() {
        val held = listOf(entry("a", 1, rev = 5), entry("b", 2, rev = 6, text = "old"))
        val merged = mergeThreadEntries(held, listOf(entry("c", 3, rev = 8), entry("b", 2, rev = 7, text = "new"), entry("a", 1, rev = 4, text = "stale")))
        assertEquals(listOf("a" to "a", "b" to "new", "c" to "c"), merged.map { it.id to it.text })
        // The computer's answer arriving after a live event doesn't undo it.
        assertEquals("new", mergeThreadEntries(listOf(entry("b", 2, rev = 7, text = "new")), listOf(entry("b", 2, rev = 6, text = "old"))).single().text)
    }

    @Test fun `live entries reach only the thread being read`() {
        val reading = mapOf("thread-1" to listOf(entry("a", 1, rev = 5)))
        val events = listOf(event("thread-1", entry("b", 2, rev = 6)), event("thread-2", entry("x", 1, rev = 7, botId = "thread-2")),
            MirrorEvent.EntryChanged(entry("chat", 9, rev = 8, botId = "b")))
        val received = reading.receivingThreads(events)
        assertEquals(setOf("thread-1"), received.keys)
        assertEquals(listOf("a", "b"), received.getValue("thread-1").map { it.id })
        // Nothing for the threads being read: the same map, so the screen isn't told of a change.
        assertSame(reading, reading.receivingThreads(events.drop(1)))
        assertTrue(emptyMap<String, List<Entry>>().receivingThreads(events).isEmpty())
    }
}
