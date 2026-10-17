package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class AnalyticsTest {
    @Test fun `the computer tag matches the host's hash`() {
        // sha256("codync-analytics:<id>")[0..8], as host/src/analytics.rs and the iPhone compute it.
        assertEquals("fbfcd4f4fe9f876c", Analytics.computerTag("tZ1WMzsVltt-fj7mUDwZsg"))
    }

    @Test fun `a crash keeps its type and Codync frames but never its message`() {
        val error = IllegalStateException("secret prompt text")
        val properties = CrashReports.properties(error).toString()
        assert("IllegalStateException" in properties)
        assert("secret prompt text" !in properties)
    }
}
