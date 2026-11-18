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

class TasksTest {
    private fun entry(id: String, seq: Long, rev: Long, text: String = id, botId: String = "task-1") =
        Entry(id, seq, botId, rev, "agent", data = buildJsonObject { put("text", text) })
    private fun event(task: String, entry: Entry) = MirrorEvent.TaskEntryChanged(task, entry)

    @Test fun `a task entry event is its own kind, never a chat's entry`() {
        val wire = Json.parseToJsonElement(
            """{"type":"taskEntry","rev":9,"task":"task-1","entry":{"id":"e1","seq":3,"botId":"task-1","rev":9,"kind":"tool","data":{"title":"Run"}}}""").jsonObject
        val event = MirrorEvent.decode(wire)
        assertEquals(MirrorEvent.TaskEntryChanged("task-1", Entry("e1", 3, "task-1", 9, "tool", data = buildJsonObject { put("title", "Run") })), event)
        assertFalse(event is MirrorEvent.EntryChanged)
    }

    @Test fun `a task entry event this app can't read is passed over`() {
        for (wire in listOf("""{"type":"taskEntry","rev":9,"entry":{"id":"e1","seq":3,"botId":"task-1","rev":9,"kind":"tool"}}""",
            """{"type":"taskEntry","rev":9,"task":"task-1"}""", """{"type":"taskEntry","rev":9,"task":"task-1","entry":{"id":""}}""")) {
            assertEquals(wire, MirrorEvent.Ignored, MirrorEvent.decode(Json.parseToJsonElement(wire).jsonObject))
        }
    }

    @Test fun `a task's entries merge once each, the higher rev winning, oldest first`() {
        val held = listOf(entry("a", 1, rev = 5), entry("b", 2, rev = 6, text = "old"))
        val merged = mergeTaskEntries(held, listOf(entry("c", 3, rev = 8), entry("b", 2, rev = 7, text = "new"), entry("a", 1, rev = 4, text = "stale")))
        assertEquals(listOf("a" to "a", "b" to "new", "c" to "c"), merged.map { it.id to it.text })
        // The computer's answer arriving after a live event doesn't undo it.
        assertEquals("new", mergeTaskEntries(listOf(entry("b", 2, rev = 7, text = "new")), listOf(entry("b", 2, rev = 6, text = "old"))).single().text)
    }

    @Test fun `live entries reach only the task being read`() {
        val reading = mapOf("task-1" to listOf(entry("a", 1, rev = 5)))
        val events = listOf(event("task-1", entry("b", 2, rev = 6)), event("task-2", entry("x", 1, rev = 7, botId = "task-2")),
            MirrorEvent.EntryChanged(entry("chat", 9, rev = 8, botId = "b")))
        val received = reading.receivingTasks(events)
        assertEquals(setOf("task-1"), received.keys)
        assertEquals(listOf("a", "b"), received.getValue("task-1").map { it.id })
        // Nothing for the tasks being read: the same map, so the screen isn't told of a change.
        assertSame(reading, reading.receivingTasks(events.drop(1)))
        assertTrue(emptyMap<String, List<Entry>>().receivingTasks(events).isEmpty())
    }
}
