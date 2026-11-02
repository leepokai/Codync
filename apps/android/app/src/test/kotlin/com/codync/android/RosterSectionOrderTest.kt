package com.codync.android

import com.codync.android.core.Bot
import org.junit.Assert.assertEquals
import org.junit.Test

class RosterSectionOrderTest {
    @Test fun sectionsMoveAcrossBotsWithTheirRowsAndPinnedBotsStayFirst() {
        val bots = listOf(Bot("pin", pinned = true), Bot("a", section = "Work"), Bot("b", section = "Work"), Bot("loose"))
        fun shown(order: List<String>) = rosterLines(bots, emptySet(), order).filterIsInstance<RosterLine.Row>().map { it.bot.id }
        assertEquals(listOf("pin", "loose", "a", "b"), shown(listOf("", "Work")))
        assertEquals(listOf("pin", "a", "b", "loose"), shown(listOf("Work", "")))
        assertEquals(listOf(RosterLine.Row(bots[0]), RosterLine.Label, RosterLine.Header("Work", 2, 0, true)),
            rosterLines(bots, emptySet(), listOf("", "Work"), moving = true))
    }
    @Test fun aHeadingDragMovesBothWaysAcrossBots() {
        val drag = SectionReorder()
        drag.names = listOf("Work", "")
        drag.heights[""] = 40
        drag.heights["Work"] = 40
        drag.position = { drag.shown.indexOf(it) * 40f }
        drag.save = { drag.names = it }
        val handle = drag.handle("Work")
        handle.start()
        handle.move(30f)
        assertEquals(listOf("", "Work"), drag.shown)
        handle.move(-40f)
        assertEquals(listOf("Work", ""), drag.shown)
        handle.end()
        assertEquals(null, drag.dragged)
    }
    @Test fun foldingKeepsTheHeadingUnderTheFingerAndCancellationDiscardsMoves() {
        val drag = SectionReorder()
        drag.names = listOf("Home", "Work", "")
        var top = 200f
        drag.position = { top }
        drag.heights[""] = 40
        var saved = false
        drag.save = { saved = true }
        val handle = drag.handle("Work")
        handle.start()
        top = 80f
        assertEquals(120f, drag.translation("Work"))
        handle.move(30f)
        assertEquals(listOf("Home", "", "Work"), drag.shown)
        handle.cancel?.invoke()
        assertEquals(false, saved)
        assertEquals(drag.names, drag.shown)
    }
    @Test fun independentComputersHaveSeparatePreferenceKeys() {
        val first = SectionOrders.key("account", "first")
        val second = SectionOrders.key("account", "second")
        val orders = mapOf(first to listOf("Work", ""), second to listOf("", "Work"))
        assertEquals(listOf("Work", ""), orders[first])
        assertEquals(listOf("", "Work"), orders[second])
    }
    @Test fun mergingIntoANewGroupKeepsItsCurrentPlace() {
        val displayed = orderedSections(listOf("Home", "Work", ""), listOf("Work", ""))
        assertEquals(listOf("", "Home"), renamedSectionOrder(displayed, "Work", "Home"))
    }
    @Test fun savedOrderIgnoresAbsentNamesAndAppendsNewGroupsOnce() {
        assertEquals(listOf("Work", "", "Home"), orderedSections(listOf("Home", "Work", ""), listOf("Work", "missing", "", "Work")))
    }
    @Test fun renameKeepsItsPlaceAndMergeKeepsDestinationPlace() {
        assertEquals(listOf("Work", ""), renamedSectionOrder(listOf("Work", ""), "Work", "Work"))
        assertEquals(listOf("Play", "", "Home"), renamedSectionOrder(listOf("Work", "", "Home"), "Work", "Play"))
        assertEquals(listOf("", "Home"), renamedSectionOrder(listOf("Work", "", "Home"), "Work", "Home"))
    }
}
