package com.codync.android.core

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement
import org.junit.Assert.*
import org.junit.Test

class AgentAccountsTest {
    private fun backend(extra: String) = WireJson.decodeFromString<AgentBackend>("""{"id":"claude","name":"Claude Code"$extra}""")

    @Test fun accountsAreAbsentFromAnOlderHostAndAnAgentWithoutThem() {
        assertNull(backend("").accounts)
        assertEquals(emptyList<AgentAccount>(), backend(""","accounts":[]""").accounts)
        val accounts = backend(""","accounts":[{"id":"a1","name":"me@example.com","signedIn":true},{"id":"a2","name":null,"signedIn":false},{"id":"a3"}]""").accounts
        assertEquals(listOf(AgentAccount("a1", "me@example.com", true), AgentAccount("a2"), AgentAccount("a3")), accounts)
    }

    @Test fun anAccountCarriesTheNameTheUserGaveItAndTheDefaultOneToo() {
        val agent = backend(""","accountSignIn":"browser","ownAccount":{"name":"Work","label":"Work","email":"me@example.com"},
            "accounts":[{"id":"a1","name":"Side","label":"Side","email":"you@example.com","signedIn":true},{"id":"a2","name":"you@example.com","label":null,"email":"you@example.com"}]""")
        assertEquals(OwnAccount("Work", "Work", "me@example.com"), agent.ownAccount)
        assertTrue(agent.signsInBrowser)
        assertEquals(listOf("Side", null), agent.accounts?.map { it.label })
        assertEquals("you@example.com", agent.accounts?.first()?.email)
        // An older host, and an agent whose CLI signs in.
        assertNull(backend("").ownAccount)
        assertFalse(backend("").signsInBrowser)
        assertFalse(backend(""","accountSignIn":"terminal","ownAccount":{}""").signsInBrowser)
    }

    @Test fun aBotPicksASignedInAccountOrKeepsItsOwn() {
        val agent = backend(""","accounts":[{"id":"a1","name":"me@example.com","signedIn":true},{"id":"a2","signedIn":false}]""")
        assertEquals(listOf("a1"), agent.accountChoices(null).map { it.id })
        assertEquals(listOf("a1", "a2"), agent.accountChoices("a2").map { it.id })
        assertEquals(emptyList<AgentAccount>(), backend("").accountChoices("gone"))
    }

    @Test fun theDraftSendsItsAccountAndAnExplicitNullForTheComputersLogin() {
        val bot = HostClient.decode(WireJson.decodeFromString("""{"id":"b","name":"Scout","backend":"claude","account":"a1"}"""))
        val draft = BotDraft.from(bot)
        assertEquals(JsonPrimitive("a1"), draft.body()["account"])
        assertEquals(JsonNull, draft.copy(account = null).body()["account"])
        assertNull(draft.withAgent("codex").account)
        assertFalse("\"account\"" in draft.template())
    }
}
