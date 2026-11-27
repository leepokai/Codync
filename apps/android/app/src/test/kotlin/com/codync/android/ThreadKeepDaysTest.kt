package com.codync.android

import com.codync.android.core.Computer
import com.codync.android.core.OlderThreads
import com.codync.android.core.WireJson
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ThreadKeepDaysTest {
    private fun hello(text: String) = WireJson.parseToJsonElement(text).jsonObject

    @Test fun anOlderHostHasNoChoice() {
        assertNull(threadKeepDays(hello("""{"version":"2.60.0"}""")))
        assertNull(threadKeepDays(null))
        assertEquals(0, threadKeepDays(hello("""{"threadKeepDays":0}""")))
        assertEquals(30, threadKeepDays(hello("""{"threadKeepDays":30}""")))
    }

    @Test fun theComputersOwnNumberIsAChoiceToo() {
        assertEquals(listOf(0 to "Keep forever", 7 to "After 7 days", 30 to "After 30 days", 90 to "After 90 days"), threadKeepChoices(30))
        assertEquals(listOf(0, 7, 14, 30, 90), threadKeepChoices(14).map { it.first })
        assertEquals("After 365 days", threadKeepChoices(365).last().second)
    }

    @Test fun theComputersEventListsOlderThreadsAgainEvenUnderTheSameChoice() {
        val listed = AppState(Computer("fixture", "Studio", "fixture"), hello = hello("""{"threadKeepDays":30}"""), olderThreads = OlderThreads(emptyList(), more = true))
        assertEquals(listed, listed.withThreadKeepDays(30))
        assertNull(listed.heardThreadKeepDays(30).olderThreads)
        // A page asked before the cleanup is let go, even when the list was not read yet.
        assertEquals(2, listed.heardThreadKeepDays(30).heardThreadKeepDays(30).olderThreadsGeneration)
        val dropped = listed.heardThreadKeepDays(30)
        assertNull(dropped.takingOlderThreads(listed.olderThreadsGeneration, OlderThreads(emptyList(), more = false)).olderThreads)
        assertEquals(false, dropped.takingOlderThreads(dropped.olderThreadsGeneration, OlderThreads(emptyList(), more = false)).olderThreads?.more)
        // A page that fails is let go (and the first page asked again) only once the list was dropped since it was asked.
        assertEquals(false, listed.droppedOlderThreadsSince(listed.olderThreadsGeneration))
        assertEquals(true, dropped.droppedOlderThreadsSince(listed.olderThreadsGeneration))
        val chosen = listed.withThreadKeepDays(7)
        assertEquals(7 to null, threadKeepDays(chosen.hello) to chosen.olderThreads)
    }

    @Test fun aLaterPageThatFailsAfterACleanupAsksTheFirstPageAgain() = runBlocking {
        val held = OlderThreads.decode(hello("""{"threads":[{"botId":"b1","id":"t1","status":"done","endedAt":200}],"more":true}"""))
        val state = MutableStateFlow(AppState(Computer("fixture", "Studio", "fixture"), hello = hello("""{"threadKeepDays":30}"""), olderThreads = held))
        val asked = mutableListOf<Long?>()
        state.readOlderThreads(held.before, state.value.olderThreadsGeneration) { before ->
            asked += before
            if (asked.size == 1) {
                state.value = state.value.heardThreadKeepDays(30)
                error("the host went away")
            }
            hello("""{"threads":[{"botId":"b1","id":"t2","status":"done","endedAt":100}],"more":false}""")
        }
        assertEquals(listOf<Long?>(200, null), asked)
        assertEquals(listOf("t2"), state.value.olderThreads?.threads?.map { it.task.id })
        // A failure with the list still held is the reader's to show.
        assertEquals("kept", runCatching { state.readOlderThreads(null, state.value.olderThreadsGeneration) { error("kept") } }.exceptionOrNull()?.message)
    }
}
