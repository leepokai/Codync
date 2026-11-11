package com.codync.android

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SupportLogTest {
    @After fun clear() = SupportLog.clear()

    @Test fun `tokens, keys and addresses are masked`() {
        assertEquals("bearer •••", SupportLog.redact("bearer abc.def.ghi"))
        assertEquals("ghp_ghp_…", SupportLog.redact("ghp_abcdef123456"))
        assertEquals("ed…@example.com", SupportLog.redact("edwin@example.com"))
        assertEquals("long abcd…", SupportLog.redact("long abcdefghijklmnopqrstuvwxyz012345"))
    }

    @Test fun `a multi-line message stays one line`() {
        assertEquals("first | second | third", SupportLog.redact("first\r\nsecond\n  third"))
    }

    @Test fun `the saved file stamps every kept line`() {
        SupportLog.clear()
        SupportLog.info(SupportSource.Connection, "Connected over relay")
        val lines = SupportLog.fileText().lines()
        assertEquals(1, lines.size)
        assertTrue(lines.single(), lines.single().contains(" INFO CONNECTION Connected over relay"))
        assertFalse(lines.single().startsWith(" INFO"))
    }

    @Test fun `the log keeps the newest 5000 events`() {
        SupportLog.clear()
        repeat(5_001) { SupportLog.info(SupportSource.App, "event $it") }
        val kept = SupportLog.events.value
        assertEquals(5_000, kept.size)
        assertEquals("event 1", kept.first().message)
        assertEquals("event 5000", kept.last().message)
    }
}
