package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class EffortLevelTest {
    private val level = """{"id":"effort","name":"Thinking","category":"thought_level","default":"low","values":[{"id":"low","name":"Low"},{"id":"high","name":"High"}]}"""
    private fun bot(fields: String): Bot = WireJson.decodeFromString(Bot.serializer(), """{"id":"b","backend":"claude"$fields}""")
    private val catalog = listOf(WireJson.decodeFromString(ModelOption.serializer(), level))

    private fun caption(saved: String?, fields: String?): String? =
        when (val row = EffortLevel.of(catalog, saved, fields?.let { bot(it).report("claude") }, loading = false, failed = false, chosenModel = false)) {
            is EffortLevel.Row -> row.caption
            else -> "no row"
        }

    @Test fun `the effort level is the thought_level setting whatever its id`() {
        val verbosity = ModelOption("verbosity", "Verbosity", category = "_custom")
        assertEquals("effort", EffortLevel.setting(listOf(verbosity) + catalog)?.id)
        assertNull(EffortLevel.setting(listOf(verbosity)))
    }

    @Test fun `a bot carries its session report and an older host reports nothing`() {
        val running = bot(""","sessionLive":true,"sessionOptions":{"effort":"high"},"sessionSettings":[$level],"settingIssues":{"effort":{"issue":"rejected","value":"max"}},"restartPending":true""")
        assertTrue(running.sessionLive && running.restartPending)
        assertEquals(mapOf("effort" to "high"), running.sessionOptions)
        assertEquals("thought_level", running.sessionSettings?.single()?.category)
        assertEquals(SettingIssue("rejected", "max"), running.settingIssues["effort"])
        val old = bot("")
        assertFalse(old.sessionLive || old.restartPending)
        assertNull(old.sessionOptions)
        assertTrue(old.settingIssues.isEmpty())
    }

    @Test fun `a step hears only its own agent`() {
        val onFallback = bot(""","agentStep":{"step":1,"agent":"codex"},"sessionLive":true""")
        assertNull(onFallback.report("claude"))
        assertNotNull(onFallback.report("codex"))
        assertNotNull(bot(""","sessionLive":true""").report("claude"))
    }

    @Test fun `the caption says what the running session confirms`() {
        assertEquals("In use: High", caption("high", ""","sessionLive":true,"sessionOptions":{"effort":"high"}"""))
        assertEquals("In use: Low (agent default)", caption(null, ""","sessionLive":true,"sessionOptions":{"effort":"low"}"""))
        assertEquals("Saved: High · not confirmed (in use: Low)", caption("high", ""","sessionLive":true,"sessionOptions":{"effort":"low"}"""))
        assertEquals("Current: unknown", caption("high", ""","sessionLive":true,"sessionOptions":{}"""))
        assertEquals("Not running · Saved: High", caption("high", null))
        assertEquals("In use: Low · High pending", caption("high", ""","sessionLive":true,"restartPending":true,"sessionOptions":{"effort":"low"}"""))
        assertEquals("Saved: High · not applied (the agent refused it)", caption("high", ""","settingIssues":{"effort":{"issue":"rejected","value":"high"}}"""))
    }

    @Test fun `an unknown or absent effort level`() {
        assertEquals(EffortLevel.Later, EffortLevel.of(emptyList(), null, null, loading = false, failed = false, chosenModel = true))
        assertEquals(EffortLevel.None, EffortLevel.of(emptyList(), null, null, loading = false, failed = false, chosenModel = false))
        assertEquals(EffortLevel.Row(null, "Saved: high · couldn’t check this agent’s options"),
            EffortLevel.of(emptyList(), "high", null, loading = false, failed = true, chosenModel = false))
    }

    @Test fun `a fallback row says what its own agent reports or what is saved without asking it`() {
        val running = bot(""","agentStep":{"step":1,"agent":"codex"},"sessionLive":true,"sessionOptions":{"effort":"high"},"sessionSettings":[$level]""").report("codex")
        assertEquals("Effort level · In use: High", EffortLevel.fallbackLine(mapOf("effort" to "high"), running))
        assertEquals("Effort level · In use: High (agent default)", EffortLevel.fallbackLine(emptyMap(), running))
        assertEquals("Saved: high", EffortLevel.fallbackLine(mapOf("effort" to "high"), null))
        assertNull(EffortLevel.fallbackLine(emptyMap(), null))
    }

    @Test fun `a step shows every setting its running session lists and any saved one nobody lists stays visible`() {
        // Greptile on #204: the session's verbosity was hidden, and a saved effort the agent stopped listing vanished.
        val verbosity = ModelOption("verbosity", "Verbosity", listOf(ModelOptionValue("terse", "Terse")))
        val verbosityJson = """{"id":"verbosity","name":"Verbosity","values":[{"id":"terse","name":"Terse"}]}"""
        val listed = bot(""","sessionSettings":[$level,$verbosityJson]""").report("claude")
        assertEquals(catalog + verbosity, EffortLevel.shown(emptyList(), listed, emptyMap()))
        val pending = bot(""","restartPending":true,"sessionSettings":[$level]""").report("claude")
        assertEquals(listOf(verbosity), EffortLevel.shown(listOf(verbosity), pending, emptyMap()))
        assertEquals(listOf(ModelOption("effort", "effort")), EffortLevel.shown(emptyList(), null, mapOf("effort" to "high")))
        assertEquals(catalog, EffortLevel.shown(catalog, null, mapOf("effort" to "high")))
    }

    @Test fun `a session that lists fewer settings than the catalog keeps the catalog's others`() {
        // Greptile on #204: Cursor lists its settings per model, while its session lists only the model.
        val verbosity = ModelOption("verbosity", "Verbosity", listOf(ModelOptionValue("terse", "Terse")))
        val quiet = bot(""","sessionSettings":[]""").report("claude")
        assertEquals(catalog + verbosity, EffortLevel.shown(catalog + verbosity, quiet, emptyMap()))
        val max = ModelOption("effort", "Thinking", listOf(ModelOptionValue("max", "Max")), category = "thought_level")
        val own = bot(""","sessionSettings":[{"id":"effort","name":"Thinking","category":"thought_level","values":[{"id":"max","name":"Max"}]}]""").report("claude")
        assertEquals(listOf(max, verbosity), EffortLevel.shown(catalog + verbosity, own, emptyMap()))
    }
}
