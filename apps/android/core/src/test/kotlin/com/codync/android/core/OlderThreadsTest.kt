package com.codync.android.core

import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class OlderThreadsTest {
    private fun json(text: String) = WireJson.parseToJsonElement(text).jsonObject
    private fun task(id: String, endedAt: Long) = WorkTask(id, "Task $id", WorkStatus.Done, endedAt = endedAt)

    @Test fun aPageReadsEachRowWithItsBot() {
        val page = OlderThreads.decode(json("""{"threads":[
            {"botId":"b1","id":"t1","title":"One","status":"done","thread":"x","endedAt":200},
            {"botId":"b2","id":"t2","status":"spinning"},
            {"id":"t3","status":"done"},
            {"botId":"plain","id":"t4","status":"failed","endedAt":100}
        ],"more":true}"""))
        assertEquals(listOf("b1" to "t1", "plain" to "t4"), page.threads.map { it.botId to it.task.id })
        assertEquals("One", page.threads.first().task.title)
        assertEquals(true, page.more)
        assertEquals(100L, page.before)
        assertFalse(OlderThreads.decode(json("""{"threads":[]}""")).more)
        assertNull(OlderThreads().before)
    }

    @Test fun pagesAddOnlyThreadsNotHeld() {
        val first = OlderThreads(listOf(OlderThread("b1", task("t1", 300)), OlderThread("b2", task("t1", 250))), more = true)
        val next = first + OlderThreads(listOf(OlderThread("b1", task("t1", 300)), OlderThread("b1", task("t2", 200))), more = false)
        assertEquals(listOf("b1" to "t1", "b2" to "t1", "b1" to "t2"), next.threads.map { it.botId to it.task.id })
        assertFalse(next.more)
        assertEquals(listOf("t1", "t2"), next.of("b1").map { it.id })
    }

    @Test fun aThreadThatLeavesItsBotsWorkStaysAsItLastReadWithoutMovingTheCursor() {
        val paged = OlderThreads(listOf(OlderThread("b1", task("t1", 50)), OlderThread("b1", task("t2", 40))), more = true)
        val reopened = WorkTask("t1", "Reopened", WorkStatus.Done, endedAt = 90)
        val live = WorkTask("t3", "Live", WorkStatus.Working)
        val kept = paged.keep("b1", listOf(reopened, live), listOf(live))
        assertEquals(listOf("Reopened", "Task t2"), kept.of("b1").map { it.title })
        assertEquals(40L, kept.before)
        assertEquals(kept, kept.keep("b1", listOf(live), listOf(live)))
    }

    @Test fun aDroppedThreadThatLeavesItsBotsWorkIsNotKept() {
        val dropped = OlderThreads(listOf(OlderThread("b1", task("t2", 40))), more = true).drop("b1", "t2").drop("b1", "t1")
        assertEquals(emptyList<WorkTask>(), dropped.keep("b1", listOf(task("t1", 90)), emptyList()).of("b1"))
    }

    @Test fun theBotsOwnRowWins() {
        val own = WorkTask("t1", "Reopened", WorkStatus.Working)
        val merged = mergedWork(listOf(own), listOf(task("t1", 100), task("t2", 50)))
        assertEquals(listOf(own, task("t2", 50)), merged)
    }

    @Test fun theChoiceFromAnotherDeviceIsHeard() {
        assertEquals(MirrorEvent.ThreadKeepDays(30), MirrorEvent.decode(json("""{"type":"threadKeepDays","days":30}""")))
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(json("""{"type":"threadKeepDays"}""")))
    }
}
