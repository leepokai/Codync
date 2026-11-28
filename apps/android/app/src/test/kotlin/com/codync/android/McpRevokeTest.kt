package com.codync.android

import com.codync.android.core.Computer
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class McpRevokeTest {
    @Test fun eachAppsRevokeIsItsOwnActionApartFromTheListsRead() {
        val reading = AppState(Computer("fixture", "Studio", "fixture")).copy(busy = setOf("mcpApps"))
        assertFalse("a read on its way never holds a revoke back", reading.revokingMcpApp("a"))
        val one = reading.copy(busy = reading.busy + "mcpRevoke:a")
        assertTrue(one.revokingMcpApp("a"))
        assertFalse("another app's revoke stays free", one.revokingMcpApp("b"))
    }
}
