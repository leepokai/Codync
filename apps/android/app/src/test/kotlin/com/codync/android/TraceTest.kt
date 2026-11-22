package com.codync.android

import com.codync.android.core.Entry
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
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

    private fun tool(id: String, parent: String? = null, turn: Long = 1) = Entry(id, 0, "bot", 0, "tool", turn, buildJsonObject {
        put("toolCallId", id)
        if (parent != null) put("parent", parent)
    })

    @Test fun `a subagent's steps sit under its launch in its turn, even when other work came between them`() {
        val steps = listOf(tool("a"), tool("b"), tool("a1", "a"), tool("b1", "b"), tool("a2", "a"), tool("lost", "gone"), tool("self", "self"), tool("later", "a", turn = 2))
        assertEquals(listOf("a" to 0, "a1" to 1, "a2" to 1, "b" to 0, "b1" to 1, "lost" to 0, "self" to 0, "later" to 0),
            nestedSteps(steps).map { (e, depth) -> e.id to depth })
    }

    @Test fun `a subagent's launch reads its kind, tokens and time once finished, or that it runs in the background`() {
        fun line(json: String) = subagentLine(Json.parseToJsonElement(json).jsonObject, tr = { it })
        assertNull(line("""{"subagent": {"kind": "general-purpose", "background": false}}"""))
        assertEquals("general-purpose · 42.3k tokens · 6 s", line("""{"subagent": {"kind": "general-purpose", "tokens": 42310, "durationMs": 6287}}"""))
        assertEquals("Explore · 812 tokens · 2 min", line("""{"subagent": {"kind": "Explore", "tokens": 812, "durationMs": 125000}}"""))
        assertEquals("general-purpose · in background", line("""{"subagent": {"kind": "general-purpose", "background": true}}"""))
        assertNull(line("""{"subagent": 3}"""))
        assertNull(line("""{}"""))
        assertEquals(listOf("1.2k tokens", "2k tokens", "123k tokens", "1.2M tokens"),
            listOf(1150, 2000, 123456, 1200000).map { line("""{"subagent": {"tokens": $it}}""") })
    }
}
