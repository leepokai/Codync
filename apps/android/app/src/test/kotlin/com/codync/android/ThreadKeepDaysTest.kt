package com.codync.android

import com.codync.android.core.Computer
import com.codync.android.core.OlderThreads
import com.codync.android.core.WireJson
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
        val chosen = listed.withThreadKeepDays(7)
        assertEquals(7 to null, threadKeepDays(chosen.hello) to chosen.olderThreads)
    }
}
