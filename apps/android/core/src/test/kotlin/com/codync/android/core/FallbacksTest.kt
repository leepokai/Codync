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
        assertEquals("Usage resets 3:00 PM", bot.restOf(Login("claude"), now = 1000)?.line("3:00 PM"))
        assertNull("a rest whose time has passed is over", bot.restOf(Login("gemini"), now = 1000))
        assertEquals("Resting until 3:00 PM", bot.resting.last().line("3:00 PM"))

        val old = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout","agentStep":null}""").jsonObject)
        assertNull(old.fallbacks); assertNull(old.selfFallbacks); assertNull(old.agentStep); assertTrue(old.resting.isEmpty())
    }

    @Test fun `bots mix agents when they run more than one, counting a fallback and leaving groups out`() {
        fun bot(json: String) = HostClient.decode(Json.parseToJsonElement(json).jsonObject)
        val claude = bot("""{"id":"a","name":"A","backend":"claude"}""")
        val onCodex = bot("""{"id":"b","name":"B","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}}""")
        val group = bot("""{"id":"g","name":"G","kind":"group","backend":"codex"}""")
        assertEquals("codex", onCodex.runningAgent)
        assertFalse(listOf(claude, bot("""{"id":"c","name":"C","backend":"claude"}"""), group).mixesAgents())
        assertTrue(listOf(claude, onCodex).mixesAgents())
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

    @Test fun `a step runs on a login profile, and rests and the step in use match by login`() {
        val bot = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout","backend":"claude","loginProfile":"a1",
            "fallbacks":[{"backend":"claude","loginProfile":"a2"},{"backend":"claude","loginProfile":null},{"backend":"codex"}],
            "agentStep":{"step":1,"agent":"claude","loginProfile":"a2","since":5},
            "resting":[{"agent":"claude","loginProfile":"a1","until":2000,"reason":"usageLimit"}]}""").jsonObject)
        assertEquals(listOf(Login("claude", "a2"), Login("claude"), Login("codex")), bot.fallbacks?.map { it.login })
        assertEquals(bot.fallbacks?.first()?.login, bot.agentStep?.login)
        assertNotNull(bot.restOf(bot.login, now = 1000))
        assertNull("another login of the same agent isn't resting", bot.restOf(Login("claude"), now = 1000))
        val draft = BotDraft.from(bot)
        assertEquals(listOf(Login("claude", "a1"), Login("claude", "a2"), Login("claude"), Login("codex")), draft.logins)
        val sent = draft.body().getValue("fallbacks").jsonArray.map { it.jsonObject["loginProfile"] }
        assertEquals(listOf(JsonPrimitive("a2"), JsonNull, JsonNull), sent)
    }

    @Test fun `the banner tells two logins of one agent apart`() {
        fun bot(step: String) = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"B","backend":"claude","agentStep":$step}""").jsonObject)
        val name = { login: Login, own: Boolean -> login.backend + (login.loginProfile?.let { " ($it)" } ?: if (own) " (Default login profile)" else "") }
        val clock = { _: Long -> "3:00 PM" }
        assertEquals("Using claude (Work) while claude (Default login profile) can't answer",
            bot("""{"step":1,"agent":"claude","loginProfile":"Work","since":1}""").stepBanner(name, clock))
        assertEquals("Using codex while claude can't answer", bot("""{"step":1,"agent":"codex","since":1}""").stepBanner(name, clock))
        assertNull(bot("null").stepBanner(name, clock))
    }

    @Test fun `a new step takes a login the chain doesn't have, signed in`() {
        val claude = WireJson.decodeFromString<AgentBackend>("""{"id":"claude","name":"Claude Code",
            "loginProfiles":[{"id":"a1","signedIn":true},{"id":"a2","signedIn":true},{"id":"a3","signedIn":false}]}""")
        assertEquals(listOf(null, "a1", "a2"), claude.freeLogins(emptyList()))
        assertEquals(listOf("a2"), claude.freeLogins(listOf(Login("claude"), Login("claude", "a1"), Login("codex", "a2"))))
        val codex = WireJson.decodeFromString<AgentBackend>("""{"id":"codex","name":"Codex"}""")
        assertEquals(listOf<String?>(null), codex.freeLogins(listOf(Login("claude"))))
        assertEquals(emptyList<String?>(), codex.freeLogins(listOf(Login("codex"))))
    }

    @Test fun `switching agent drops only the step on that agent's own login`() {
        val draft = BotDraft(backend = "claude", loginProfile = "a1", fallbacks = listOf(Fallback("codex"), Fallback("codex", loginProfile = "c1"), Fallback("claude")))
        val switched = draft.withAgent("codex")
        assertNull(switched.loginProfile)
        assertEquals(listOf(Login("codex", "c1"), Login("claude")), switched.fallbacks?.map { it.login })
    }
}
