package com.codync.android

import com.codync.android.core.Entry
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TraceTest {
    private fun entry(id: String, turn: Long, status: String? = null, kind: String = "user") =
        Entry(id, 0, "bot", 0, kind, turn, buildJsonObject { put("text", id); if (status != null) put("status", status) })

    @Test fun `the trace unfolds the running turn, not a queued message holding the next`() {
        val entries = listOf(entry("u1", 1), entry("t1", 1), entry("q", 2, status = "queued"))
        assertEquals(1L, runningTurn(entries, working = true))
        assertNull(runningTurn(entries, working = false))
    }
}
