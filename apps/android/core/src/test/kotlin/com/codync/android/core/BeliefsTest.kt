package com.codync.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class BeliefsTest {
    private fun belief(file: String, kind: String = "user", confidence: String = "medium", confirmed: String? = null) =
        Belief(file, kind = kind, confidence = confidence, confirmed = confirmed)

    @Test fun `the host's list decodes and extra fields are ignored`() {
        val json = """{"beliefs":[{"file":"home.md","shared":false,"description":"Where","kind":"project","confidence":"low","confirmed":null,
            "updatedAt":1,"tokens":9,"versions":[{"id":3,"content":null,"cause":"delete","at":30,"rev":1},{"id":2,"content":"x","cause":"edit","at":20}]}]}"""
        val b = Belief.decodeAll(Json.parseToJsonElement(json)).single()
        assertEquals(Triple("home.md", "project", "low"), Triple(b.file, b.kind, b.confidence))
        assertEquals(listOf(Triple(3L, "Forgotten", false), Triple(2L, "Changed", true)), b.historyRows().map { Triple(it.id, it.label, it.restorable) })
    }

    @Test fun `filters keep what matches and null means all`() {
        val all = listOf(belief("a"), belief("b", "feedback", "low"), belief("c", "project", "high"))
        assertEquals(3, Belief.filter(all, null, null).size)
        assertEquals(listOf("b"), Belief.filter(all, "feedback", null).map { it.file })
        assertEquals(listOf("c"), Belief.filter(all, null, "high").map { it.file })
        assertEquals(0, Belief.filter(all, "user", "low").size)
    }

    @Test fun `the unsure and the long unconfirmed come first`() {
        val sorted = Belief.byAttention(listOf(
            belief("sure", confidence = "high", confirmed = "2026-10-01"), belief("old", confirmed = "2026-01-01"),
            belief("new", confirmed = "2026-10-01"), belief("doubt", confidence = "low", confirmed = "2026-10-30")))
        assertEquals(listOf("doubt", "old", "new", "sure"), sorted.map { it.file })
    }

    @Test fun `labels follow the other clients`() {
        assertEquals("How to work", Belief.kindLabel("feedback"))
        assertEquals("Unsure", Belief.confidenceLabel("low"))
        assertEquals("Never confirmed", Belief.confirmedText(null))
        assertEquals("Confirmed 2026-10-30", Belief.confirmedText("2026-10-30"))
    }
}
