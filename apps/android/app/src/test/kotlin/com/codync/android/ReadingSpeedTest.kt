package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingSpeedTest {
    @Test fun `speeds read in the sheet's own words`() {
        assertEquals(listOf("0.5×", "0.75×", "1×", "1.25×", "1.5×", "2×"), readingSpeeds.map(::speedLabel))
    }
}
