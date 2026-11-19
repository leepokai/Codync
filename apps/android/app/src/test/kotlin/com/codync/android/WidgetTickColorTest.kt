package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class WidgetTickColorTest {
    private val tint = 0xFFD97757.toInt()
    private val red = 0xFFC23A2B.toInt()
    private val muted = 0xFF6B6B6B.toInt()

    @Test fun staleReadingsStayMutedEvenAtHighPercent() {
        assertEquals(muted, tickColor(95.0, tint, red, muted, stale = true))
        assertEquals(muted, tickColor(40.0, tint, red, muted, stale = true))
        assertEquals(red, tickColor(95.0, tint, red, muted, stale = false))
        assertEquals(tint, tickColor(40.0, tint, red, muted, stale = false))
        assertEquals(muted, tickColor(Double.NaN, tint, red, muted, stale = true))
    }
}
