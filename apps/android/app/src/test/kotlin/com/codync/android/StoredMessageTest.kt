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

    @Test fun `a line break at either edge of the search half keeps exact pieces`() {
        fun textWithBreak(at: Int) = "x".repeat(80).toCharArray().also { it[at] = '\n' }.concatToString()
        for ((at, firstEnd) in listOf(19 to 40, 20 to 21, 39 to 40)) {
            val text = textWithBreak(at)
            val ranges = text.storedChunkRanges(40)
            assertEquals("break at $at", firstEnd, ranges.first().last - ranges.first().first + 1)
            assertEquals("break at $at", text, ranges.joinToString("") { text.substring(it) })
        }
    }

    @Test(timeout = 5_000) fun `a giant single line is cut once in linear time`() {
        val text = "e".repeat(2_000_000)
        val ranges = text.storedChunkRanges()
        assertEquals(500, ranges.size)
        assertTrue(ranges.all { it.last - it.first + 1 == STORED_CHUNK_CHARS })
        assertEquals(0, ranges.first().first)
        assertEquals(text.length - 1, ranges.last().last)
    }
}
