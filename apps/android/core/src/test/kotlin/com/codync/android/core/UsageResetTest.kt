package com.codync.android.core

import java.time.ZonedDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UsageResetTest {
    private val now = ZonedDateTime.parse("2026-10-17T22:25:00+08:00[Asia/Taipei]").toInstant().toEpochMilli()
    private fun at(text: String) = ZonedDateTime.parse(text).toInstant().toEpochMilli()

    @Test fun `Claude Code reset texts read as the next such moment`() {
        assertEquals(at("2026-10-18T01:10:00+08:00[Asia/Taipei]"), UsageWindow.parseReset("Oct 18 at 1:10am (Asia/Taipei)", now))
        assertEquals(at("2026-10-24T12:00:00+08:00[Asia/Taipei]"), UsageWindow.parseReset("Oct 24 at 12pm (Asia/Taipei)", now))
        // A bare time already past today is tomorrow; a date long past is next year.
        assertEquals(at("2026-10-18T07:30:00+08:00[Asia/Taipei]"), UsageWindow.parseReset("7:30am (Asia/Taipei)", now))
        assertEquals(at("2027-01-02T09:00:00+08:00[Asia/Taipei]"), UsageWindow.parseReset("Jan 2 at 9am (Asia/Taipei)", now))
        assertNull(UsageWindow.parseReset("soon", now))
    }
}
