package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AnnouncementTest {
    private val all = listOf(Announcement("2.46.0", listOf("C")), Announcement("2.45.0", listOf("B")), Announcement("2.44.0", listOf("A")))

    private fun due(seen: String?, current: String, usedBefore: Boolean = true) =
        Announcement.due(all, seen, current, usedBefore).map { it.version }

    @Test fun `shows the releases since the last one seen, newest first`() {
        assertEquals(listOf("2.45.0", "2.44.0"), due("2.43.2", "2.45.1"))
        assertEquals(listOf("2.46.0", "2.45.0"), due("2.44.0", "2.46.0"))
        assertEquals(emptyList<String>(), due("2.46.0", "2.46.0"))
        assertEquals("a downgrade shows nothing", emptyList<String>(), due("2.47.0", "2.46.0"))
    }

    @Test fun `without a record it depends on whether the phone was set up`() {
        assertEquals("a new install", emptyList<String>(), due(null, "2.44.0", usedBefore = false))
        assertEquals("an update from before the record", listOf("2.44.0"), due(null, "2.44.0", usedBefore = true))
        assertEquals("an unreadable version", emptyList<String>(), due("2.43.0", "", usedBefore = true))
    }

    @Test fun `reads the shared file`() {
        val text = """[{"version": "2.44.0", "notes": ["A", "B"]}]"""
        assertEquals(listOf(Announcement("2.44.0", listOf("A", "B"))), Announcement.parse(text))
    }

    @Test fun `records at once when nothing is due, else once shown`() {
        var seen: String? = null
        val events = mutableListOf<String>()
        fun run(current: String, usedBefore: Boolean = false) =
            WhatsNewRun(all, seen, current, usedBefore, save = { seen = it }, capture = { events += "whats_new_shown" })

        assertTrue(run("2.44.0").due.isEmpty())
        assertEquals("2.44.0", seen)

        val update = run("2.46.0")
        assertEquals(listOf("2.46.0", "2.45.0"), update.due.map { it.version })
        assertEquals("not before it's on screen", "2.44.0", seen)
        update.shown()
        update.shown()
        assertEquals("2.46.0", seen)
        assertEquals(listOf("whats_new_shown"), events)

        assertTrue(run("2.46.0", usedBefore = true).due.isEmpty())
    }
}
