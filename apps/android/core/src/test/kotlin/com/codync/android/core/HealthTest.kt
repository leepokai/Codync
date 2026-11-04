package com.codync.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HealthTest {
    private val gb = 1024L * 1024 * 1024
    private fun health(level: String = "warning", refusing: Boolean = false, used: Int = 75) =
        HostHealth(ramTotal = 8 * gb, ramFree = 2 * gb, ramUsedPercent = used, cpu = 12.4, diskTotal = 500 * gb, diskFree = 120 * gb,
            level = level, refusing = refusing)

    @Test fun `the host's answer decodes and extra fields are ignored`() {
        val json = """{"ramTotal":8589934592,"ramFree":2147483648,"ramUsedPercent":75,"cpu":12.4,"diskTotal":0,"diskFree":0,"level":"warning",
            "refusing":false,"thresholds":{"warn":70,"critical":85},"agents":[{"botId":"a","botName":"Big","pid":4,"rss":2147483648}],"at":1,"new":true}"""
        val h = HostHealth.decode(Json.parseToJsonElement(json))
        assertEquals(75, h.ramUsedPercent)
        assertEquals(HealthLevels(70, 85), h.thresholds)
        assertEquals(listOf("Big" to "2.0 GB"), h.agentRows())
    }

    @Test fun `bytes are told in the unit a person reads`() {
        assertEquals("1.5 GB", HostHealth.bytesText((1.5 * gb).toLong()))
        assertEquals("320 MB", HostHealth.bytesText(320L * 1024 * 1024))
        assertEquals("0 MB", HostHealth.bytesText(0))
    }

    @Test fun `the lines come in the order memory cpu disk`() {
        assertEquals(listOf("Memory" to "75 % used · 2.0 GB free of 8.0 GB", "CPU" to "12 %", "Disk" to "120.0 GB free of 500.0 GB"), health().lines())
        assertEquals("Unknown", health().copy(diskTotal = 0).lines()[2].second)
    }

    @Test fun `the small alert is silent when all is well and says why turns stop when they do`() {
        assertNull(health("normal", used = 40).alert())
        assertEquals("Memory 75 % · getting full", health().alert())
        assertEquals("Memory 90 % · not starting new turns", health("critical", true, 90).alert())
        assertEquals("Memory is full", HostHealth.levelLabel("critical"))
        assertEquals("Healthy", HostHealth.levelLabel("normal"))
    }

    @Test fun `agents are listed biggest first`() {
        val h = health().copy(agents = listOf(HealthAgent("a", "Small", 1, 200L * 1024 * 1024), HealthAgent("b", "Big", 2, 2 * gb)))
        assertEquals(listOf("Big" to "2.0 GB", "Small" to "200 MB"), h.agentRows())
    }
}
