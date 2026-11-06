package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class QrPixelsTest {
    @Test
    fun `a code fills a square with black and white pixels`() {
        val pixels = QrPixels.encode("codync://pair?code=abc", 64)

        assertEquals(64 * 64, pixels.size)
        assertTrue(pixels.all { it == 0xFF000000.toInt() || it == 0xFFFFFFFF.toInt() })
        assertTrue(pixels.any { it == 0xFF000000.toInt() })
    }
}
