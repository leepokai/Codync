package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhonePerformanceTest {
    @Test fun `chat opening reports elapsed milliseconds only once`() {
        var now = 1_000_000_000L
        val timer = OpenToMessages { now }
        timer.begin()
        now += 1_234_000_000
        assertEquals(1234L, timer.finish())
        now += 1_000_000_000
        assertNull(timer.finish())
    }

    @Test fun `chat frame before opening reports nothing`() {
        assertNull(OpenToMessages().finish())
    }

    @Test fun `chat opening uses host diagnostic wire shape`() {
        assertEquals("""{"type":"chat_open","platform":"android","durationMs":1234}""",
            PhonePerformance.ChatOpen(1234).body().toString())
    }
}
