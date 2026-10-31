package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerSelectionTest {
    @Test fun `the filter shows several computers or only one`() {
        val all = listOf("a", "b", "c")
        val initial = ComputerSelection(all, "")
        assertEquals(all, initial.shown)
        val withoutB = ComputerSelection(all, initial.toggling("b"))
        assertEquals(listOf("a", "c"), withoutB.shown)
        val onlyC = ComputerSelection(all, withoutB.only("c"))
        assertEquals(listOf("c"), onlyC.shown)
        assertEquals(listOf("a", "c"), ComputerSelection(all, onlyC.toggling("a")).shown)
    }

    @Test fun `one computer always shows, removed ones don't hide the rest, and newly paired ones show`() {
        val single = ComputerSelection(listOf("a", "b"), "b")
        assertEquals(listOf("a"), ComputerSelection(single.all, single.toggling("a")).shown)
        val removed = ComputerSelection(listOf("b", "c"), "b,c")
        assertEquals(listOf("b", "c"), removed.shown)
        assertEquals(listOf("c"), ComputerSelection(removed.all, removed.toggling("b")).shown)
        assertEquals(listOf("a", "new"), ComputerSelection(listOf("a", "b", "new"), "b").shown)
        assertTrue(ComputerSelection(emptyList(), "a").shown.isEmpty())
        assertEquals("", ComputerSelection(listOf("a"), "a").only("gone"))
    }
}
