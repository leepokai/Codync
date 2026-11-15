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
    private fun bot(tasks: String?) =
        HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Bot","rev":1${tasks?.let { ""","tasks":$it""" }.orEmpty()}}""").jsonObject)
    private fun task(status: TaskStatus = TaskStatus.Working, activity: String = "", title: String = "Fix login", id: String = "task-1") =
        BotTask(id, title, status, activity, startedAt = 1)
    private fun entry(id: String, seq: Long, rev: Long, text: String = id, botId: String = "task-1") =
        Entry(id, seq, botId, rev, "agent", data = buildJsonObject { put("text", text) })
    private fun event(task: String, entry: Entry) = MirrorEvent.TaskEntryChanged(task, entry)

    @Test fun `a bot's tasks decode, and survive the mirror's copy`() {
        val bot = bot("""[{"id":"task-1","title":"Fix login","status":"needsInput","activity":"Reading","startedAt":5,"start":"fork"}]""")
        assertEquals(listOf(BotTask("task-1", "Fix login", TaskStatus.NeedsInput, "Reading", 5)), bot.tasks)
        assertEquals(bot, HostClient.decode(Json.parseToJsonElement(WireJson.encodeToString(Bot.serializer(), bot)).jsonObject))
    }

    @Test fun `a host from before tasks has none`() {
        assertEquals(emptyList<BotTask>(), bot(null).tasks)
    }

    @Test fun `tasks this app can't read never lose the bot`() {
        assertEquals(emptyList<BotTask>(), bot("null").tasks)
        assertEquals(emptyList<BotTask>(), bot(""""soon"""").tasks)
        assertEquals(emptyList<BotTask>(), bot("""{"id":"task-1"}""").tasks)
        // A task without an id is left out; one with fields missing, null or a status it doesn't know still shows.
        val mixed = bot("""[7, {"title":"No id"}, {"id":"task-1","title":null,"startedAt":null}, {"id":"task-2"}, {"id":"task-3","status":"paused","title":"New"}]""")
        assertEquals(listOf(BotTask("task-1"), BotTask("task-2"), BotTask("task-3", "New", TaskStatus.Working)), mixed.tasks)
    }

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

    @Test fun `one task says what it is doing`() {
        assertEquals(TasksSummary.One(TaskWhat("Needs you", phrase = true)), tasksSummary(listOf(task(TaskStatus.NeedsInput, "Reading"))))
        assertEquals(TasksSummary.One(TaskWhat("Starting…", phrase = true)), tasksSummary(listOf(task(TaskStatus.Starting))))
        assertEquals(TasksSummary.One(TaskWhat("Reading", phrase = true)), tasksSummary(listOf(task(activity = "Reading"))))
        // With no step yet, its title, which is the bot's own words.
        assertEquals(TasksSummary.One(TaskWhat("Fix login", phrase = false)), tasksSummary(listOf(task())))
    }

    @Test fun `several tasks say how many, and whether one needs the user`() {
        assertEquals(TasksSummary.Several(2, needsInput = false), tasksSummary(listOf(task(), task(TaskStatus.Starting, id = "task-2"))))
        assertEquals(TasksSummary.Several(3, needsInput = true),
            tasksSummary(listOf(task(), task(TaskStatus.NeedsInput, id = "task-2"), task(id = "task-3"))))
    }

    @Test fun `only the tasks still going are live`() {
        assertEquals(listOf(TaskStatus.Starting, TaskStatus.Working, TaskStatus.NeedsInput), TaskStatus.entries.filter { it.live })
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
