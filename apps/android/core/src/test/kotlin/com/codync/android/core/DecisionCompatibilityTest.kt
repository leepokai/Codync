package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class DecisionCompatibilityTest {
    @Serializable private data class LegacyFields(val text: String? = null, val options: List<PermissionChoice>? = null)

    @Test fun `choice notices decode without changing the old permission field type`() {
        val wire = WireJson.parseToJsonElement("""{"id":"e","seq":1,"botId":"b","rev":1,"turn":1,"kind":"notice","data":{"decision":true,"text":"Ship?\n1. Ship\n2. Wait","question":"Ship?","choices":["Ship","Wait"],"recommended":1,"status":"pending"}}""").jsonObject
        val entry = Entry.decode(wire)
        assertTrue(entry.isChat)
        assertTrue(entry.isDecision)
        val legacy = WireJson.decodeFromJsonElement<LegacyFields>(wire.getValue("data"))
        assertNull(legacy.options)
        assertTrue(legacy.text!!.contains("2. Wait"))
    }

    @Test fun `older notices are not choice cards`() {
        val wire = WireJson.parseToJsonElement("""{"id":"e","seq":1,"botId":"b","rev":1,"kind":"notice","data":{"text":"Renamed the bot"}}""").jsonObject
        val entry = Entry.decode(wire)
        assertTrue(entry.isChat)
        assertFalse(entry.isDecision)
    }
}
