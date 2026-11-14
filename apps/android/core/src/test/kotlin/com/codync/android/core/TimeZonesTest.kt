package com.codync.android.core

import java.time.Instant
import org.junit.Assert.*
import org.junit.Test

class TimeZonesTest {
    @Test fun `search ignores case and separators`() {
        assertEquals("Asia/Taipei", TimeZones.search("TAIPEI").first())
        assertEquals("Asia/Taipei", TimeZones.search("asia tai").first())
        assertEquals("America/New_York", TimeZones.search("new york").first())
        assertEquals("America/Port-au-Prince", TimeZones.search("port au").first())
        assertTrue("Asia/Taipei" in TimeZones.search("tpe"))
        assertEquals(TimeZones.ids, TimeZones.search("  "))
        assertTrue(TimeZones.search("zzzz").isEmpty())
    }

    @Test fun `prefix matches come before word starts, then anywhere, then letters in order`() {
        val zones = listOf("Asia/Hind", "America/Indiana", "Indian/Chagos", "Iran/Nod")
        assertEquals(listOf("Indian/Chagos", "America/Indiana", "Asia/Hind", "Iran/Nod"), TimeZones.search("ind", zones))
    }

    @Test fun `the list keeps IANA zones and drops Java and SystemV aliases`() {
        assertEquals(listOf("Asia/Taipei", "EST", "UTC"), TimeZones.iana(listOf("UTC", "ACT", "SystemV/AST4", "EST", "Asia/Taipei", "CTT")))
    }

    @Test fun `offsets read as GMT hours and minutes`() {
        val winter = Instant.parse("2026-01-15T00:00:00Z")
        assertEquals("GMT+8", TimeZones.offset("Asia/Taipei", winter))
        assertEquals("GMT+5:30", TimeZones.offset("Asia/Kolkata", winter))
        assertEquals("GMT-3:30", TimeZones.offset("America/St_Johns", winter))
        assertEquals("GMT-5", TimeZones.offset("America/New_York", winter))
        assertEquals("GMT", TimeZones.offset("UTC", winter))
        assertEquals("", TimeZones.offset("Not/AZone", winter))
    }
}
