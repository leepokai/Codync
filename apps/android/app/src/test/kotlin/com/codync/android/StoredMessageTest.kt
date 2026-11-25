package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class StoredMessageTest {
    @Test fun `a message that fits is one piece`() {
        assertEquals(listOf(0 until 5), "hello".storedChunkRanges(10))
    }

    @Test fun `a long message is cut into pieces of its size and loses nothing`() {
        val text = (1..100).joinToString("\n") { "line $it" }
        val ranges = text.storedChunkRanges(40)
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
        assertTrue(ranges.size > 1)
        assertTrue(ranges.all { it.last - it.first + 1 <= 40 })
    }

    @Test fun `a piece ends at a line break when one comes in its second half`() {
        val text = "a".repeat(30) + "\n" + "b".repeat(60)
        val ranges = text.storedChunkRanges(40)
        assertEquals("a".repeat(30) + "\n", text.substring(ranges.first()))
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
    }

    @Test fun `a long line is cut at its size`() {
        val text = "c".repeat(95)
        val ranges = text.storedChunkRanges(40)
        assertEquals(listOf(40, 40, 15), ranges.map { it.last - it.first + 1 })
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
    }

    @Test fun `the pieces cover the message once with no empty one at the end`() {
        val text = "d".repeat(80)
        val ranges = text.storedChunkRanges(40)
        assertEquals(listOf(0 until 40, 40 until 80), ranges)
    }
}
