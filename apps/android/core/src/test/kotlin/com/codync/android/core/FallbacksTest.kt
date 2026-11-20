package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

/** The banner as English reads it: the phrase with its names put in. */
private fun Bot.stepBanner(name: (Login, own: Boolean) -> String, clock: (Long) -> String, now: Long = System.currentTimeMillis()) =
    stepBannerPhrase(name, clock, now)?.let { fill(it.key, it.values) }

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

    @Test fun `only a usage limit gives the banner a reset time, other rests name their reason`() {
        fun resting(reason: String) = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"B","backend":"claude",
            "agentStep":{"step":1,"agent":"codex","since":1},"resting":[{"agent":"claude","until":2000,"reason":"$reason"}]}""").jsonObject)
        val name = { login: Login, _: Boolean -> login.backend }
        val clock = { _: Long -> "3:00 PM" }
        assertEquals("Using codex until claude resets at 3:00 PM", resting("usageLimit").stepBanner(name, clock, now = 1000))
        for (reason in listOf("signedOut", "rateLimited", "outOfCredits", "overloaded", "notStarting")) {
            assertFalse(reason, resting(reason).stepBanner(name, clock, now = 1000).orEmpty().contains("resets"))
        }
        assertEquals("Using codex while claude is signed out", resting("signedOut").stepBanner(name, clock, now = 1000))
    }

    @Test fun `every banner the host can cause has its phrase in the catalog`() {
        val catalog = Json.parseToJsonElement(java.io.File("../../ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hant.json").readText()).jsonObject
        // The reasons `rest::record` rests an agent for, and none; a missing model or mode never rests one.
        for (reason in listOf(null, "usageLimit", "outOfCredits", "rateLimited", "overloaded", "signedOut", "notStarting")) {
            val rest = reason?.let { ""","resting":[{"agent":"claude","until":2000,"reason":"$it"}]""" }.orEmpty()
            val bot = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"B","backend":"claude","agentStep":{"step":1,"agent":"codex","since":1}$rest}""").jsonObject)
            val key = bot.stepBannerPhrase({ login, _ -> login.backend }, { "3:00 PM" }, now = 1000)!!.key
            assertTrue("$reason: $key", key in catalog)
        }
    }

    @Test fun `a login profile name holding while stays one name`() {
        val bot = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"B","backend":"claude","loginProfile":"Work while traveling",
            "agentStep":{"step":1,"agent":"codex","since":1},
            "resting":[{"agent":"claude","loginProfile":"Work while traveling","until":2000,"reason":"signedOut"}]}""").jsonObject)
        val name = { login: Login, own: Boolean -> login.backend + (login.loginProfile?.let { " ($it)" } ?: if (own) " (Default login profile)" else "") }
        val clock = { _: Long -> "3:00 PM" }
        val phrase = bot.stepBannerPhrase(name, clock, now = 1000)
        // The phrase keeps its slot, and the whole name is one value: translating the phrase never sees the name.
        assertEquals("Using {to} while {own} is signed out", phrase?.key)
        assertEquals(mapOf("to" to "codex", "own" to "claude (Work while traveling)"), phrase?.values)
        assertEquals("Using codex while claude (Work while traveling) is signed out", phrase?.let { fill(it.key, it.values) })
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

    @Test fun `a step picked in place gets its own back on its own agent`() {
        val claude = WireJson.decodeFromString<AgentBackend>("""{"id":"claude","name":"Claude Code",
            "loginProfiles":[{"id":"a1","signedIn":true},{"id":"a2","signedIn":false}]}""")
        val codex = WireJson.decodeFromString<AgentBackend>("""{"id":"codex","name":"Codex"}""")
        val taken = listOf(Login("claude"), Login("codex"))
        assertEquals(Fallback("claude", loginProfile = "a1"), claude.stepOn(taken))
        val own = Fallback("claude", model = "opus", modelOptions = mapOf("effort" to "high"), loginProfile = "a2")
        assertEquals(own, claude.stepOn(taken, own))
        // Unless another step has its login now.
        assertEquals("a1", claude.stepOn(taken + own.login, own).loginProfile)
        // Its signed-out login stays a choice, and so does its agent.
        assertEquals(listOf("a1", "a2"), claude.stepLogins(own.login, taken))
        val full = taken + Login("claude", "a1")
        assertEquals(emptyList<AgentBackend>(), listOf(claude, codex).usable(full))
        assertEquals(listOf("claude"), listOf(claude, codex).usable(full, listOf(own.login)).map { it.id })
        // Its remembered login is gone once another step has it and no other is free: picking that agent would land on the bot's own login.
        assertEquals(emptyList<AgentBackend>(), listOf(claude, codex).usable(full + own.login, listOf(own.login)))
    }

    @Test fun `switching agent drops only the step on that agent's own login`() {
        val draft = BotDraft(backend = "claude", loginProfile = "a1", fallbacks = listOf(Fallback("codex"), Fallback("codex", loginProfile = "c1"), Fallback("claude")))
        val switched = draft.withAgent("codex")
        assertNull(switched.loginProfile)
        assertEquals(listOf(Login("codex", "c1"), Login("claude")), switched.fallbacks?.map { it.login })
    }
}
