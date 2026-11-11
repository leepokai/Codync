package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ResidentBotsTest {
    @Test fun `a chat window never shrinks below the default and never passes the ceiling`() {
        val residency = Residency(setOf("a"), windows = mapOf("a" to 500, "b" to 10, "c" to 1_000_000))
        assertEquals(500, residency.limit("a"))
        assertEquals(Residency.DEFAULT_WINDOW, residency.limit("b"))
        assertEquals(Residency.DEFAULT_WINDOW, residency.limit("none"))
        assertEquals(Residency.MAX_WINDOW, residency.limit("c"))
    }

    @Test fun `earlier rows continue only while the shown chat is under the ceiling`() {
        assertTrue(Residency.needsEarlier(0, false))
        assertTrue(Residency.needsEarlier(Residency.MAX_WINDOW - 1, false))
        assertFalse(Residency.needsEarlier(Residency.MAX_WINDOW, false))
        assertFalse(Residency.needsEarlier(0, true))
    }

    @Test fun `a sixth open chat drops the least recently used`() {
        val residents = ResidentBots()
        repeat(6) { residents.touch("b$it") }
        assertEquals((1..5).map { "b$it" }.toSet(), residents.full(null))
    }

    @Test fun `opening a chat again keeps it and drops the one that became oldest`() {
        val residents = ResidentBots(capacity = 2)
        residents.touch("a")
        residents.touch("b")
        residents.touch("a")
        assertEquals(setOf("b", "a"), residents.full(null))
        residents.touch("c")
        assertEquals(setOf("a", "c"), residents.full(null))
    }

    @Test fun `the open chat is resident even before it is remembered`() {
        val residents = ResidentBots(capacity = 2)
        residents.touch("a")
        residents.touch("b")
        assertEquals(setOf("a", "b", "c"), residents.full("c"))
    }

    @Test fun `blank ids take no slot`() {
        val residents = ResidentBots(capacity = 1)
        residents.touch("")
        residents.touch(" ")
        assertEquals(emptySet<String>(), residents.full(" "))
        assertEquals(setOf("a"), residents.full("a"))
    }

    @Test fun `a zero capacity keeps only the open chat`() {
        val residents = ResidentBots(capacity = 0)
        residents.touch("a")
        assertEquals(emptySet<String>(), residents.full(null))
        assertEquals(setOf("a"), residents.full("a"))
    }
}
