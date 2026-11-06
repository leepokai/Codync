package com.codync.android

import com.codync.android.core.Bot
import org.junit.Assert.*
import org.junit.Test

class RosterMoveTest {
    private val roster = listOf(Bot("pin", pinned = true), Bot("a"), Bot("b"), Bot("c"))

    @Test fun moveUpAndDownSwapWithTheNeighbor() {
        assertEquals(listOf("pin", "b", "a", "c"), moved(roster, "b", -1))
        assertEquals(listOf("pin", "a", "c", "b"), moved(roster, "b", 1))
    }

    @Test fun aBotStaysAmongBotsWithItsPinStateAndInsideTheList() {
        assertNull(moved(roster, "a", -1))
        assertNull(moved(roster, "pin", 1))
        assertNull(moved(roster, "c", 1))
        assertNull(moved(roster, "missing", 1))
    }

    @Test fun theLeadComesFirstAndStaysThere() {
        val bots = listOf(Bot("pin", pinned = true, lastAt = 9), Bot("a", lastAt = 5), Bot("lead", lead = true, position = 3, lastAt = 1))
        val sorted = roster(bots)
        assertEquals(listOf("lead", "pin", "a"), sorted.map(Bot::id))
        assertNull(moved(sorted, "lead", 1))
        assertNull(moved(sorted, "pin", -1))
    }
}
