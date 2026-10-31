package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Test

class DictationTextTest {
    @Test fun `words go after the box with one space`() {
        assertEquals("hello", DictationText.join("  ", "hello"))
        assertEquals("Fix it", DictationText.join("Fix it", ""))
        assertEquals("Fix it now", DictationText.join("Fix it", "now"))
        assertEquals("Fix it\nnow", DictationText.join("Fix it\n", "now"))
    }

    @Test fun `a recording goes in 20 s pieces, a short tail staying with the last`() {
        val piece = DictationText.PIECE_BYTES
        assertEquals(listOf(0 until 100), DictationText.pieces(100))
        assertEquals(listOf(0 until piece, piece until piece + DictationText.MIN_BYTES), DictationText.pieces(piece + DictationText.MIN_BYTES))
        assertEquals(listOf(0 until piece + 10), DictationText.pieces(piece + 10))
        assertEquals(emptyList<IntRange>(), DictationText.pieces(0))
    }
}
