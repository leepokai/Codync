package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UsageSpentTest {
    private val now = 1_800_000_000_000L
    private fun window(id: String, percent: Double, resetsAt: Long? = now + 60_000, text: String? = null) =
        UsageWindow(id, id, percent, resetsAt, text = text)

    @Test fun `a spent window stops its own agent's models`() {
        val report = UsageReport(listOf(
            UsageProvider("claude:work", "Claude", listOf(window("five_hour", 100.0))),
            UsageProvider("codex", "Codex", listOf(window("primary", 40.0))),
        ))
        assertEquals("five_hour", report.spentWindow("claude", now)?.id)
        assertNull(report.spentWindow("codex", now))
        assertNull(report.spentWindow("gemini", now))
    }

    @Test fun `a source names the agent too`() {
        val report = UsageReport(listOf(UsageProvider("antigravity", "Antigravity", listOf(window("weekly", 120.0)))))
        assertEquals("weekly", report.spentWindow("antigravity", now)?.id)
    }

    @Test fun `an unspent or already reset window is not a stop`() {
        val report = UsageReport(listOf(UsageProvider("codex", "Codex", listOf(
            window("primary", 99.0),
            window("secondary", 100.0, resetsAt = now - 1),
        ))))
        assertNull(report.spentWindow("codex", now))
    }

    @Test fun `a note that is not a limit never counts`() {
        val report = UsageReport(listOf(UsageProvider("codex", "Codex", listOf(
            window("tokens", 100.0, text = "1.2M tokens"),
        ))))
        assertNull(report.spentWindow("codex", now))
    }

    @Test fun `the fullest spent window is the one shown`() {
        val report = UsageReport(listOf(UsageProvider("claude", "Claude", listOf(
            window("weekly", 100.0),
            window("five_hour", 160.0),
        ))))
        assertEquals("five_hour", report.spentWindow("claude", now)?.id)
    }
}
