package com.codync.android

import com.codync.android.core.Entry
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TraceTest {
    private fun entry(id: String, turn: Long, status: String? = null, kind: String = "user", text: String = id,
                      final: Boolean? = null, bookkeeping: Boolean? = null) =
        Entry(id, 0, "bot", 0, kind, turn, buildJsonObject {
            put("text", text)
            if (status != null) put("status", status)
            if (final != null) put("final", final)
            if (bookkeeping != null) put("bookkeeping", bookkeeping)
        })

    @Test fun `the trace unfolds the running turn, not a queued message holding the next`() {
        val entries = listOf(entry("u1", 1), entry("t1", 1), entry("q", 2, status = "queued"))
        assertEquals(1L, runningTurn(entries, working = true))
        assertNull(runningTurn(entries, working = false))
    }

    @Test fun `the working steps are the running turn's work, without what the chat or memory already show`() {
        val entries = listOf(
            entry("old", 1, kind = "tool"), entry("u2", 2), entry("thinking", 2, kind = "thought"),
            entry("blank", 2, kind = "thought", text = "  "), entry("says", 2, kind = "agent"),
            entry("terminal", 2, status = "in_progress", kind = "tool", bookkeeping = false),
            entry("memory", 2, kind = "tool", bookkeeping = true), entry("unflagged", 2, kind = "tool"),
            entry("plan", 2, kind = "plan"), entry("reply", 2, kind = "agent", final = true), entry("card", 2, kind = "permission"),
            entry("pass", 2, kind = "agent", text = "(pass)"), entry("q", 3, status = "queued"),
        )
        assertEquals(listOf("thinking", "says", "terminal", "plan"), workingSteps(entries).map { it.id })
    }
}
