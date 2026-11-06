package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class FallbacksTest {
    @Test fun `a bot decodes its fallbacks, the step it runs on and its resting agents, and an older host's without them`() {
        val bot = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout","backend":"claude",
            "fallbacks":[{"backend":"codex","model":"gpt-5.5","modelOptions":{"effort":"high"}},{"backend":"gemini","model":null}],
            "selfFallbacks":false,"agentStep":{"step":1,"agent":"codex","model":"gpt-5.5","since":5,"reason":"usageLimit"},
            "resting":[{"agent":"claude","until":2000,"reason":"usageLimit"},{"agent":"gemini","until":500,"reason":"signedOut"}]}""").jsonObject)
        assertEquals(listOf(Fallback("codex", model = "gpt-5.5", modelOptions = mapOf("effort" to "high")), Fallback("gemini")), bot.fallbacks)
        assertEquals(false, bot.selfFallbacks)
        assertEquals("codex", bot.agentStep?.agent)
        assertEquals("Usage resets 3:00 PM", bot.restOf("claude", now = 1000)?.line("3:00 PM"))
        assertNull("a rest whose time has passed is over", bot.restOf("gemini", now = 1000))
        assertEquals("Resting until 3:00 PM", bot.resting.last().line("3:00 PM"))

        val old = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout","agentStep":null}""").jsonObject)
        assertNull(old.fallbacks); assertNull(old.selfFallbacks); assertNull(old.agentStep); assertTrue(old.resting.isEmpty())
    }

    @Test fun `fallbacks move one place within the list, never past either end`() {
        val list = listOf(Fallback("codex"), Fallback("gemini"), Fallback("opencode"))
        assertEquals(listOf("gemini", "codex", "opencode"), list.moved(1, -1).map { it.backend })
        assertEquals(listOf("codex", "opencode", "gemini"), list.moved(1, 1).map { it.backend })
        assertSame(list, list.moved(0, -1))
        assertSame(list, list.moved(2, 1))
    }

    @Test fun `the draft sends the whole list, leaves it out for an older host, and drops a fallback that becomes the bot's agent`() {
        val draft = BotDraft.from(Bot("b", "Scout", backend = "claude", fallbacks = listOf(Fallback("codex"), Fallback("gemini")), selfFallbacks = true))
        val sent = draft.body().getValue("fallbacks").jsonArray.map { it.jsonObject }
        assertEquals(listOf("codex", "gemini"), sent.map { it["backend"]?.jsonPrimitive?.content })
        assertEquals(JsonObject(emptyMap()), sent.first()["modelOptions"])
        assertEquals(JsonPrimitive(true), draft.body()["selfFallbacks"])
        assertEquals(listOf("gemini"), draft.withAgent("codex").fallbacks?.map { it.backend })
        assertEquals("codex", draft.withAgent("codex").backend)
        // A custom command a bot set goes back as it came.
        val custom = draft.copy(fallbacks = listOf(Fallback("custom", command = "my-agent --acp")))
        assertEquals("my-agent --acp", custom.body().getValue("fallbacks").jsonArray.single().jsonObject["command"]?.jsonPrimitive?.content)
        assertFalse(WireJson.parseToJsonElement(draft.template()).jsonObject.keys.any { it in setOf("fallbacks", "selfFallbacks") })

        val old = BotDraft.from(Bot("b", "Scout", backend = "claude"))
        assertFalse("fallbacks" in old.body() || "selfFallbacks" in old.body())
        assertNull(old.withAgent("codex").fallbacks)
    }
}
