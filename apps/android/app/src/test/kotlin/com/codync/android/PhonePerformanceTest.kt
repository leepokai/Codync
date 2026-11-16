package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhonePerformanceTest {
    @Test fun `chat opening runs from its action to its own first messages once`() {
        var now = 1_000_000_000L
        val timer = OpenToMessages { now }
        timer.open("b1")
        now += 100_000_000
        assertNull(timer.finish("b2"))
        now += 1_134_000_000
        assertEquals(1234L, timer.finish("b1"))
        now += 1_000_000_000
        assertNull(timer.finish("b1"))
    }

    @Test fun `leaving or reopening a chat does not report a second opening`() {
        var now = 1_000_000_000L
        val timer = OpenToMessages { now }
        timer.open("b1")
        now += 500_000_000
        timer.open("b1")
        now += 734_000_000
        assertEquals(1234L, timer.finish("b1"))
        timer.open("b1")
        timer.open(null)
        assertNull(timer.finish("b1"))
    }

    @Test fun `chat opening uses host diagnostic wire shape`() {
        assertEquals("""{"type":"chat_open","platform":"android","durationMs":1234}""",
            PhonePerformance.ChatOpen(1234).body().toString())
    }

    @Test fun `open timing uses host diagnostic wire shape`() {
        assertEquals("""{"type":"open_timing","platform":"android","phase":"catch_up","open":"foreground","ms":1250,"chats":8,"messages":31}""",
            PhonePerformance.OpenStep(OpenPhase.CatchUp, OpenKind.Foreground, 1250, chats = 8, messages = 31).body().toString())
    }
}
