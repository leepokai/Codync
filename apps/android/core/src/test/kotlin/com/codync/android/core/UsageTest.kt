package com.codync.android.core

import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class UsageTest {
    @Test fun `the chooser list decodes and an older host sends none`() {
        val report = UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[],"services":[{"id":"cursor","name":"Cursor","shown":false}]}"""))
        assertEquals(listOf(UsageService("cursor", "Cursor", false)), report.services)
        assertEquals("Cursor", report.services.single().provider.name)
        val named = UsageReport.decode(WireJson.parseToJsonElement("""{"services":[{"id":"copilot","name":"GitHub Copilot","shown":true,"registry":"github-copilot-cli"}]}"""))
        assertEquals("github-copilot-cli", named.services.single().provider.registry)
        assertTrue(UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[]}""")).services.isEmpty())
    }
    @Test fun `a move on its way holds the order and agents it does not name follow`() {
        val report = UsageReport(services = listOf("claude", "codex", "cursor").map { UsageService(it, it, true) })
        assertEquals(listOf("claude", "codex", "cursor"), report.servicesIn(null).map { it.id })
        assertEquals(listOf("cursor", "claude", "codex"), report.servicesIn(listOf("cursor", "gone", "claude")).map { it.id })
        assertTrue(UsageReport().servicesIn(listOf("claude")).isEmpty())
    }
    @Test fun `only an agent that is on can lack a reading`() {
        // Codex is listed only for a login profile that hasn't been read.
        val report = UsageReport(providers = listOf(
            UsageProvider("claude", "Claude", listOf(UsageWindow("w", "Weekly", 10.0))),
            UsageProvider("codex", "Codex", loginProfiles = listOf(UsageLoginProfile("work", "Work")))))
        assertFalse(report.hasNoReading(UsageService("claude", "Claude", true)))
        assertTrue(report.hasNoReading(UsageService("codex", "Codex", true)))
        assertTrue(report.hasNoReading(UsageService("cursor", "Cursor", true)))
        assertFalse(report.hasNoReading(UsageService("cursor", "Cursor", false)))
    }
    @Test fun `unknown providers and missing windows remain distinct from zero consumption`() {
        val report = UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[{"id":"future","name":"Future agent","source":"future-source","windows":[{"id":"unknown","label":"Monthly"},{"id":"zero","label":"Daily","percent":0}]}]}"""))
        val provider = report.providers.single()
        assertEquals("future-source", provider.source)
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
    }

    @Test fun `login profiles decode with their readings and an agent with one login sends none`() {
        val one = UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[{"id":"claude","name":"Claude","windows":[{"id":"five_hour","label":"5-hour","percent":10}]}]}""")).providers.single()
        assertNull(one.defaultLoginProfile)
        assertTrue(one.loginProfiles.isEmpty())
        assertEquals(listOf(UsageLogin(null, null, one.windows)), one.logins)
        val many = UsageReport.decode(WireJson.parseToJsonElement("""{"providers":[{"id":"claude","name":"Claude","windows":[],"source":"claude","updatedAt":1,
            "defaultLoginProfile":"me@example.com","loginProfiles":[{"id":"p1","name":"Work","windows":[{"id":"five_hour","label":"5-hour","percent":90}],"updatedAt":0},{"id":"p2","name":"Side"}]}]}""")).providers.single()
        assertEquals(listOf(null to "me@example.com", "p1" to "Work", "p2" to "Side"), many.logins.map { it.id to it.name })
        assertEquals(listOf(0, 1, 0), many.logins.map { it.windows.size })
        assertEquals(many, WireJson.decodeFromString<UsageProvider>(WireJson.encodeToString(many)))
    }

    @Test fun `the login nearest a limit leads and a note never counts`() {
        val work = UsageLoginProfile("p1", "Work", listOf(UsageWindow("five_hour", "5-hour", 90.0), UsageWindow("seven_day", "Weekly", 40.0)))
        val provider = UsageProvider("claude", "Claude", listOf(UsageWindow("five_hour", "5-hour", 10.0)), loginProfiles = listOf(work))
        assertEquals("p1", provider.tightestLogin.id)
        assertEquals(90.0, provider.tightest?.percent)
        assertEquals(listOf("five_hour", "seven_day"), provider.limits.map { it.id })
        val noted = provider.copy(loginProfiles = listOf(UsageLoginProfile("p2", "Key", listOf(UsageWindow("cost", "Cost today", 99.0, text = "$1.50")))))
        assertNull(noted.tightestLogin.id)
        assertEquals(10.0, noted.tightest?.percent)
        val unread = UsageProvider("codex", "Codex", loginProfiles = listOf(UsageLoginProfile("p3", "Work")))
        assertNull(unread.tightestLogin.id)
        assertNull(unread.tightest)
    }
}
