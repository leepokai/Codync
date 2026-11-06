package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.addJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class EntryVisibilityTest {
    private fun entry(kind: String, hidden: Boolean? = true, id: String = kind, seq: Long = 1): Entry =
        Entry(id, seq, "bot", rev = seq, kind = kind, turn = seq, data = buildJsonObject {
            put("text", "Scheduled work")
            put("final", true)
            put("bookkeeping", false)
            put("status", "pending")
            hidden?.let { put("internal", it) }
        })

    @Test fun `internal work never renders in chat trace or reply controls`() {
        for (kind in listOf("user", "agent", "thought", "tool", "plan", "notice", "permission")) {
            val hidden = entry(kind)
            assertFalse(kind, hidden.isChat)
            assertFalse(kind, hidden.isWorkingStep)
            assertFalse(kind, hidden.isReplyable)
        }
    }

    @Test fun `older hosts and explicitly promoted output stay visible without text heuristics`() {
        val legacy = entry("user", hidden = null)
        val promoted = entry("agent", hidden = false)
        val literal = legacy.copy(data = JsonObject(legacy.data + ("text" to JsonPrimitive("NO_UPDATE"))))
        assertTrue(legacy.isChat)
        assertTrue(promoted.isChat)
        assertTrue(promoted.isReplyable)
        assertEquals(listOf(legacy, promoted, literal), listOf(entry("thought"), legacy, promoted, literal).visibleEntries())
    }

    @Test fun `cached internal metadata survives decoding and is filtered again`() {
        val raw = """{"id":"u","seq":1,"botId":"bot","rev":4,"kind":"user","data":{"text":"Scheduled work","internal":true}}"""
        val cached = Entry.decode(WireJson.parseToJsonElement(raw).jsonObject)
        assertTrue(cached.isInternal)
        assertTrue(listOf(cached).visibleEntries().isEmpty())
    }

    @Test fun `legacy scheduled inputs stay private without hiding ordinary literal replies`() {
        val ordinary = entry("user", hidden = null)
        val scheduled = ordinary.copy(data = JsonObject(ordinary.data + ("from" to JsonPrimitive("proactive"))))
        assertFalse(scheduled.isChat)
        assertFalse(scheduled.isReplyable)
        assertTrue(listOf(scheduled).visibleEntries().isEmpty())
        assertTrue(ordinary.isChat)
        assertTrue(entry("permission", hidden = false).isChat)
    }

    @Test fun `hidden decisions and replies cannot add or resolve visible pending incidents`() {
        val incident = entry("notice", hidden = false, id = "error").copy(data = buildJsonObject {
            put("text", "Visible error"); put("style", "error")
        })
        val internalReply = entry("agent", seq = 2)
        val internalPermission = entry("permission", seq = 3)
        assertEquals(listOf("error"), collectPendingDecisions(listOf(incident, internalReply, internalPermission)).map { it.entryId })
    }

    @Test fun `internal files and call entries stay out of secondary conversations`() {
        val hidden = entry("agent").copy(data = buildJsonObject {
            put("internal", true); put("final", true); put("text", "NO_UPDATE"); put("call", "c")
            putJsonArray("files") { addJsonObject { put("id", "f"); put("name", "private.txt"); put("size", 1); put("sha256", "") } }
        })
        val line = entry("notice", hidden = false).copy(data = buildJsonObject { put("call", "c") })
        assertTrue(collectDeliverables(listOf(hidden)).isEmpty())
        assertTrue(listOf(hidden).hidingCalls().isEmpty())
        assertTrue(listOf(hidden).callConversation(line).isEmpty())
        assertTrue(listOf(hidden).withCallEntries(listOf(hidden.copy(data = buildJsonObject { put("text", "older visible copy") }))).isEmpty())
    }

    @Test fun `a call says only a promoted public result`() {
        // The computer decides what a call hears (chat::calls::announce); the mirror still hides internal rows, so a
        // scheduled reply never reaches the call's entries before the host promotes it.
        val hidden = entry("agent")
        assertTrue(listOf(hidden).visibleEntries().isEmpty())
        val promoted = hidden.copy(rev = 2, data = JsonObject(hidden.data + ("internal" to JsonPrimitive(false))))
        assertEquals(listOf(promoted), listOf(promoted).visibleEntries())
    }
}
