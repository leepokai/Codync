package com.codync.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkTest {
    private fun bot(fields: String = "") = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Bot","rev":1$fields}""").jsonObject)
    private fun work(json: String) = bot(""","work":$json""").work
    private fun item(id: String, status: WorkStatus) = WorkItem(id, status = status)

    @Test fun `a bot's work decodes, and survives the mirror's copy`() {
        val bot = bot(""","maxAgents":4,"work":[
            {"id":"w1","title":"Fix login","status":"working","activity":"Reading","task":"task-1","wait":null,"cutIn":false,"createdAt":3,"startedAt":5,"endedAt":null,"result":"","brief":"x"},
            {"id":"w2","title":"Docs","status":"waiting","task":null,"wait":{"kind":"behind","ahead":2},"cutIn":true,"createdAt":4},
            {"id":"w3","title":"CI","status":"failed","task":"task-0","createdAt":1,"startedAt":2,"endedAt":9,"result":"No network"}]""")
        assertEquals(listOf(
            WorkItem("w1", "Fix login", WorkStatus.Working, "Reading", "task-1", createdAt = 3, startedAt = 5),
            WorkItem("w2", "Docs", WorkStatus.Waiting, wait = WorkWait(WaitKind.Behind, 2), cutIn = true, createdAt = 4),
            WorkItem("w3", "CI", WorkStatus.Failed, task = "task-0", createdAt = 1, startedAt = 2, endedAt = 9, result = "No network")), bot.work)
        assertEquals(4, bot.maxAgents)
        assertEquals(bot, HostClient.decode(Json.parseToJsonElement(WireJson.encodeToString(Bot.serializer(), bot)).jsonObject))
    }

    @Test fun `a host from before the box has no work and no ceiling`() {
        assertEquals(emptyList<WorkItem>(), bot().work)
        assertNull(bot().maxAgents)
        assertNull(BotDraft.from(bot()).maxAgents)
        assertFalse("maxAgents" in BotDraft.from(bot()).body())
    }

    @Test fun `work this app can't read never loses the bot`() {
        assertEquals(emptyList<WorkItem>(), work("null"))
        assertEquals(emptyList<WorkItem>(), work(""""soon""""))
        assertEquals(emptyList<WorkItem>(), work("""{"id":"w1","status":"working"}"""))
        // An item without an id or in a status this app doesn't know is left out; one with fields missing or null still shows.
        val mixed = work("""[7, {"title":"No id","status":"working"}, {"id":"w1","status":"paused"}, {"id":"w2"},
            {"id":"w3","status":"done","title":null,"startedAt":null}, {"id":"w4","status":"waiting"}]""")
        assertEquals(listOf(item("w3", WorkStatus.Done), item("w4", WorkStatus.Waiting)), mixed)
    }

    @Test fun `a reason to wait this app can't read is none`() {
        fun wait(json: String) = work("""[{"id":"w1","status":"waiting","wait":$json}]""").single().wait
        assertEquals(WorkWait(WaitKind.Next), wait("""{"kind":"next"}"""))
        assertEquals(WorkWait(WaitKind.Behind, 1), wait("""{"kind":"behind"}"""))
        assertEquals(WorkWait(WaitKind.Usage), wait("""{"kind":"usage","ahead":null}"""))
        for (malformed in listOf("\"next\"", """{"kind":"soon"}""", "{}", "7", """{"kind":"behind","ahead":"two"}""")) assertNull(malformed, wait(malformed))
    }

    @Test fun `the ceiling stays within one to six, and one this app can't read is none`() {
        assertEquals(1, bot(""","maxAgents":0""").maxAgents)
        assertEquals(6, bot(""","maxAgents":9""").maxAgents)
        assertEquals(3, bot(""","maxAgents":3.0""").maxAgents)
        for (malformed in listOf("null", "\"3\"", "true", "[3]", "{}")) assertNull(malformed, bot(""","maxAgents":$malformed""").maxAgents)
    }

    @Test fun `the ceiling is saved with the bot's other settings`() {
        val draft = BotDraft.from(bot(""","maxAgents":3""")).copy(maxAgents = 5)
        assertEquals("5", draft.body().getValue("maxAgents").toString())
    }

    @Test fun `items fall into threads, the box and recent, in the order their rows show`() {
        val work = listOf(item("a", WorkStatus.Working), item("b", WorkStatus.NeedsYou), item("c", WorkStatus.Working), item("d", WorkStatus.Waiting),
            item("e", WorkStatus.Waiting), item("f", WorkStatus.Stopped), item("g", WorkStatus.Done), item("h", WorkStatus.Failed))
        val sections = workSections(work, 4)
        assertEquals(listOf("b", "a", "c"), sections.threads.map { it.id })
        assertEquals(listOf("d", "e"), sections.box.map { it.id })
        assertEquals(listOf("f", "g", "h"), sections.recent.map { it.id })
        // Three run of four: one place is free.
        assertEquals(1 to 4, sections.free to sections.places)
        // More running than the ceiling (it was just lowered): none free, and the heading says what is true, `3 / 2`.
        assertEquals(0 to 2, workSections(work, 2).let { it.free to it.places })
        // A host that sends no ceiling counts as three.
        assertEquals(WorkSections(emptyList(), 3, emptyList(), emptyList()), workSections(emptyList(), null))
        assertEquals(0 to 3, workSections(work, null).let { it.free to it.places })
    }

    @Test fun `only the items a task has are live`() {
        assertEquals(listOf(WorkStatus.Working, WorkStatus.NeedsYou), WorkStatus.entries.filter { it.live })
        assertEquals(listOf(WorkStatus.Done, WorkStatus.Failed, WorkStatus.Stopped), WorkStatus.entries.filter { it.ended })
    }

    @Test fun `cutting in is for when every place is taken`() {
        val two = listOf(item("a", WorkStatus.Working), item("b", WorkStatus.NeedsYou), item("c", WorkStatus.Waiting), item("d", WorkStatus.Done))
        assertTrue(canCutIn(two, 2))
        assertFalse(canCutIn(two, 3))
        // A host that sends no ceiling counts as three.
        assertFalse(canCutIn(two, null))
        assertTrue(canCutIn(two + item("e", WorkStatus.Working), null))
        // A freed place wouldn't start one that follows unfinished work or waits for usage.
        for (kind in listOf(WaitKind.After, WaitKind.Usage)) {
            val held = item("c", WorkStatus.Waiting).copy(wait = WorkWait(kind))
            assertFalse(canCutIn(listOf(item("a", WorkStatus.Working), held), 1))
        }
    }

    @Test fun `a row offers what its item can take, the action that shows first`() {
        val work = listOf(item("a", WorkStatus.Working), item("b", WorkStatus.NeedsYou), item("c", WorkStatus.Waiting), item("d", WorkStatus.Waiting),
            item("e", WorkStatus.Done), item("f", WorkStatus.Failed), item("g", WorkStatus.Stopped))
        fun actions(id: String, maxAgents: Int? = 3) = workActions(work.first { it.id == id }, work, maxAgents)
        assertEquals(listOf(WorkAction.Stop), actions("a"))
        assertEquals(listOf(WorkAction.Stop), actions("b"))
        // The first in line can only be dropped, until every place is taken: then it can cut in.
        assertEquals(listOf(WorkAction.Drop), actions("c"))
        assertEquals(listOf(WorkAction.CutIn, WorkAction.Drop), actions("c", maxAgents = 2))
        assertEquals(listOf(WorkAction.First, WorkAction.Drop), actions("d", maxAgents = 2))
        assertEquals(listOf(WorkAction.Drop), actions("e"))
        assertEquals(listOf(WorkAction.Retry, WorkAction.Drop), actions("f"))
        assertEquals(listOf(WorkAction.Retry, WorkAction.Drop), actions("g"))
        assertEquals(listOf(WorkAction.Stop, WorkAction.Drop), WorkAction.entries.filter { it.destructive })
    }
}
