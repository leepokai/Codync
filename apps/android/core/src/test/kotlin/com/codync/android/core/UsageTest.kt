package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class UsageTest {
    @Test fun `unknown providers and missing windows remain distinct from zero consumption`() {
        val report = UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[{"id":"future","name":"Future agent","source":"future-source","windows":[{"id":"unknown","label":"Monthly"},{"id":"zero","label":"Daily","percent":0}]}]}"""))
        val provider = report.providers.single()
        assertEquals("future-source", provider.sourceLabel)
        assertNull(provider.updatedAt)
        assertNull(provider.windows.first().reportedPercent)
        assertNull(provider.windows.first().barFraction)
        assertEquals("zero", provider.tightest?.id)
        assertEquals(0.0, provider.windows.last().reportedPercent!!, 0.0)
        val noted = UsageProvider("vibe", "Mistral Vibe", listOf(
            UsageWindow("cost", "Cost today", 90.0, text = "$1.50"),
            UsageWindow("five_hour", "5-hour", 10.0)))
        assertEquals(listOf("five_hour"), noted.limits.map { it.id })
        assertEquals("five_hour", noted.tightest?.id)
        assertTrue(UsageReport.decode(WireJson.parseToJsonElement("{}" )).providers.isEmpty())
    }

    @Test fun `bars clamp drawing but retain the reported value and stale timestamp`() {
        val window = UsageWindow("session", "Session", 103.5)
        assertEquals(1f, window.barFraction!!, 0f)
        assertEquals(103.5, window.reportedPercent!!, 0.0)
        assertNull(window.copy(percent = Double.NaN).barFraction)
        val provider = UsageProvider("codex", "Codex", listOf(window, window.copy(id = "week", percent = 15.0)), "sessions", 1)
        assertEquals(1L, provider.updatedAt)
        assertEquals("session", provider.tightest?.id)
        assertEquals("Codex sessions", provider.sourceLabel)
    }
}
