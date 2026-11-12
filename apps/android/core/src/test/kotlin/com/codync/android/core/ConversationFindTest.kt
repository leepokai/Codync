package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The same cases as iOS `ConversationFindTests`. */
class ConversationFindTest {
    private fun matches(vararg messages: Pair<String, String>) = ConversationFind.matches(messages.toList(), "needle")

    @Test fun `matching ignores case and counts each place once`() {
        val messages = listOf("a" to "Week week WEEK", "b" to "aaaa", "c" to "nothing")
        assertEquals(listOf("a", "a", "a"), ConversationFind.matches(messages, "week").map { it.id })
        assertEquals(listOf(ConversationFind.Match("b", 0), ConversationFind.Match("b", 1)), ConversationFind.matches(messages, "aa"))
        assertEquals(emptyList<ConversationFind.Match>(), ConversationFind.matches(messages, "  "))
    }

    @Test fun `the find steps around and keeps its match as earlier messages load`() {
        var find = ConversationFind()
        assertEquals("", find.label(emptyList(), "No results"))
        find = find.type("needle")
        assertEquals("No results", find.label(emptyList(), "No results"))
        var found = matches("x" to "needle", "y" to "needle, needle")
        assertEquals("1/3", find.label(found, "No results"))
        find = find.step(false, found)
        assertEquals("back from the first wraps to the last", "3/3", find.label(found, "No results"))
        find = find.step(true, found).step(true, found)
        assertEquals("2/3", find.label(found, "No results"))
        // An earlier message loads above: the same match stays shown, further down the count.
        found = matches("w" to "a needle", "x" to "needle", "y" to "needle, needle")
        assertEquals("3/4", find.label(found, "No results"))
        assertNull(find.type("needles").shown)
    }

    @Test fun `the first match found stays shown as earlier matches load`() {
        val find = ConversationFind().type("needle").pinned(matches("x" to "needle"))
        assertEquals("1/1", find.label(matches("x" to "needle"), "No results"))
        assertEquals("2/2", find.label(matches("w" to "a needle", "x" to "needle"), "No results"))
        assertEquals(find, find.pinned(matches("w" to "a needle")))
    }

    @Test fun `each searched text is reported once`() {
        var find = ConversationFind()
        assertNull(find.searched())
        find = find.type("plan").searched()!!
        assertNull(find.searched())
        assertEquals("plans", find.type("plans").searched()?.query)
    }
}
