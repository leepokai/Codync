package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ResidentBotsTest {
    @Test fun `a chat window never shrinks below the default and never passes the ceiling`() {
        val residency = Residency(setOf("a"), windows = mapOf("a" to 200, "b" to 10, "c" to 1_000_000))
        assertEquals(200, residency.limit("a"))
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
        val residents = ResidentBots(5)
        repeat(6) { residents.touch("b$it") }
        assertEquals((1..5).map { "b$it" }.toSet(), residents.full(null))
    }

    @Test fun `the default set keeps only the open chat`() {
        val residents = ResidentBots()
        residents.touch("a")
        residents.touch("b")
        assertEquals(emptySet<String>(), residents.full(null))
        assertEquals(setOf("b"), residents.full("b"))
    }

    @Test fun `remembered ids stay a preview order when nothing else is resident`() {
        val residents = ResidentBots()
        repeat(10) { residents.touch("b$it") }
        assertEquals(emptySet<String>(), residents.full(null))
        assertEquals(setOf("b9"), residents.full("b9"))
        assertEquals((9 downTo 2).map { "b$it" }, residents.previewPriority())
        assertTrue(residents.previewPriority().none { it in residents.full(null) })
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

    @Test fun `a thread keeps its bot resident until its last view closes`() {
        val residents = ResidentBots()
        residents.hold("thread-owner")
        assertEquals(setOf("thread-owner"), residents.full(null))
        assertEquals(setOf("thread-owner", "another-chat"), residents.full("another-chat"))
        residents.release("thread-owner")
        assertEquals(emptySet<String>(), residents.full(null))
        assertEquals(setOf("another-chat"), residents.full("another-chat"))
    }

    @Test fun `a held thread owner does not enlarge the recently opened capacity`() {
        val residents = ResidentBots(capacity = 2)
        residents.touch("a")
        residents.touch("b")
        residents.touch("c")
        residents.hold("thread-owner")
        assertEquals(setOf("b", "c", "thread-owner"), residents.full(null))
        residents.release("thread-owner")
        assertEquals(setOf("b", "c"), residents.full(null))
    }

    @Test fun `closing one of two threads does not release their shared owner`() {
        val residents = ResidentBots()
        val threads = ThreadViews(residents)
        assertTrue(threads.watch("bot", "thread-1"))
        assertTrue(threads.watch("bot", "thread-2"))
        assertEquals(setOf("bot"), residents.full(null))
        assertTrue("thread 1's transcript can go", threads.close("bot", "thread-1"))
        assertEquals("thread 2 still needs its owner", setOf("bot"), residents.full(null))
        assertTrue(threads.close("bot", "thread-2"))
        assertEquals(emptySet<String>(), residents.full(null))
    }

    @Test fun `two views of one thread release its owner only after both close`() {
        val residents = ResidentBots()
        val threads = ThreadViews(residents)
        assertTrue(threads.watch("bot", "thread"))
        assertFalse(threads.watch("bot", "thread"))
        assertFalse("the second view is still open", threads.close("bot", "thread"))
        assertEquals(setOf("bot"), residents.full(null))
        assertTrue(threads.close("bot", "thread"))
        assertEquals(emptySet<String>(), residents.full(null))
        assertFalse("a repeated close changes nothing", threads.close("bot", "thread"))
        assertEquals(emptySet<String>(), residents.full(null))
    }

    @Test fun `a reopened thread holds its owner again`() {
        val residents = ResidentBots()
        val threads = ThreadViews(residents)
        assertTrue(threads.watch("bot", "thread"))
        assertTrue(threads.close("bot", "thread"))
        assertEquals(emptySet<String>(), residents.full(null))
        assertTrue(threads.watch("bot", "thread"))
        assertEquals(setOf("bot"), residents.full(null))
        assertTrue(threads.close("bot", "thread"))
        assertEquals(emptySet<String>(), residents.full(null))
    }
}
