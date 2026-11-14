package com.codync.android.core

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import org.junit.Assert.*
import org.junit.Test

class LoginProfilesTest {
    private fun backend(extra: String) = WireJson.decodeFromString<AgentBackend>("""{"id":"claude","name":"Claude Code"$extra}""")

    @Test fun loginProfilesAreAbsentFromAnOlderHost() {
        assertNull(backend("").loginProfiles)
        assertEquals(emptyList<LoginProfile>(), backend(""","loginProfiles":[]""").loginProfiles)
        val profiles = backend(""","loginProfiles":[{"id":"a1","kind":"account","name":"me@example.com","signedIn":true,"vars":[]},{"id":"a2","name":null,"signedIn":false},{"id":"a3"}]""").loginProfiles
        assertEquals(listOf(LoginProfile("a1", "me@example.com", true), LoginProfile("a2"), LoginProfile("a3")), profiles)
    }

    @Test fun anApiKeyProfileCarriesItsVariableNames() {
        val profiles = backend(""","loginProfiles":[{"id":"k1","kind":"apiKey","name":"Work key","signedIn":true,"vars":["ANTHROPIC_API_KEY","ANTHROPIC_BASE_URL"]},
            {"id":"k2","kind":"apiKey","signedIn":false,"vars":[]},{"id":"a1","kind":"account"},{"id":"n1","kind":"aNewerHostsKind"}]""").loginProfiles.orEmpty()
        assertEquals(listOf(LoginProfileKind.ApiKey, LoginProfileKind.ApiKey, LoginProfileKind.Account, LoginProfileKind.Account), profiles.map { it.kind })
        assertEquals("\"apiKey\"", WireJson.encodeToString(LoginProfileKind.ApiKey))
        assertEquals(listOf("ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL"), profiles[0].vars)
        assertEquals(emptyList<String>(), profiles[1].vars)
    }

    @Test fun anAgentWithOneSignInOfItsOwnTakesOnlyApiKeys() {
        val agent = backend(""","loginProfiles":[],"defaultLoginProfile":{},"loginProfileSignIn":null,"loginKeys":["OPENAI_API_KEY"]""")
        assertNull(agent.loginProfileSignIn)
        assertFalse(agent.signsInBrowser)
        assertEquals(listOf("OPENAI_API_KEY"), agent.loginKeys)
        assertEquals(emptyList<String>(), backend("").loginKeys)
        assertEquals("terminal", backend(""","loginProfileSignIn":"terminal"""").loginProfileSignIn)
    }

    @Test fun theKeyFormStartsFromTheProfilesVariablesThenTheAgentsUsualOnes() {
        val agent = backend(""","loginKeys":["OPENAI_API_KEY"]""")
        assertEquals(listOf(KeyRow("A", saved = true), KeyRow("B", saved = true)), agent.keyRows(LoginProfile("k", kind = LoginProfileKind.ApiKey, vars = listOf("A", "B"))))
        assertEquals(listOf(KeyRow("OPENAI_API_KEY", named = true)), agent.keyRows(LoginProfile("k", kind = LoginProfileKind.ApiKey)))
        assertEquals(listOf(KeyRow()), backend("").keyRows(LoginProfile("k", kind = LoginProfileKind.ApiKey)))
        assertFalse(KeyRow().named)
    }

    @Test fun saveSendsTypedValuesAndAnEmptyOneForARemovedVariable() {
        val rows = listOf(KeyRow("A", saved = true), KeyRow("B", "two", saved = true), KeyRow(" C ", "three"), KeyRow("", "lost"), KeyRow("D"))
        assertEquals(mapOf("B" to "two", "C" to "three"), rows.keysToSave())
        assertEquals(mapOf("OLD" to "", "B" to "two", "C" to "three"), rows.keysToSave(listOf("OLD")))
        // A removed name typed again keeps the new value.
        assertEquals(mapOf("B" to "two", "C" to "three"), rows.keysToSave(listOf("B")))
        assertTrue(listOf(KeyRow("A", saved = true)).keysToSave().isEmpty())
    }

    @Test fun aLoginProfileCarriesTheNameTheUserGaveItAndTheDefaultOneToo() {
        val agent = backend(""","loginProfileSignIn":"browser","defaultLoginProfile":{"name":"Work","label":"Work","email":"me@example.com"},
            "loginProfiles":[{"id":"a1","name":"Side","label":"Side","email":"you@example.com","signedIn":true},{"id":"a2","name":"you@example.com","label":null,"email":"you@example.com"}]""")
        assertEquals(DefaultLoginProfile("Work", "Work", "me@example.com"), agent.defaultLoginProfile)
        assertTrue(agent.signsInBrowser)
        assertEquals(listOf("Side", null), agent.loginProfiles?.map { it.label })
        assertEquals("you@example.com", agent.loginProfiles?.first()?.email)
        // An older host, and an agent whose CLI signs in.
        assertNull(backend("").defaultLoginProfile)
        assertFalse(backend("").signsInBrowser)
        assertFalse(backend(""","loginProfileSignIn":"terminal","defaultLoginProfile":{}""").signsInBrowser)
    }

    @Test fun aBotPicksASignedInLoginProfileOrKeepsItsOwn() {
        val agent = backend(""","loginProfiles":[{"id":"a1","name":"me@example.com","signedIn":true},{"id":"a2","signedIn":false}]""")
        assertEquals(listOf("a1"), agent.loginProfileChoices(null).map { it.id })
        assertEquals(listOf("a1", "a2"), agent.loginProfileChoices("a2").map { it.id })
        assertEquals(emptyList<LoginProfile>(), backend("").loginProfileChoices("gone"))
    }

    @Test fun theDraftSendsItsLoginProfileAndAnExplicitNullForTheComputersLogin() {
        val bot = HostClient.decode(WireJson.decodeFromString("""{"id":"b","name":"Scout","backend":"claude","loginProfile":"a1"}"""))
        val draft = BotDraft.from(bot)
        assertEquals(JsonPrimitive("a1"), draft.body()["loginProfile"])
        assertEquals(JsonNull, draft.copy(loginProfile = null).body()["loginProfile"])
        assertNull(draft.withAgent("codex").loginProfile)
        assertFalse("\"loginProfile\"" in draft.template())
    }
}
