package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class NetworkTest {
    private val view = WireJson.parseToJsonElement("""
        {"joined": true, "assistants": false, "assistantsAvailable": true, "networks": [
          {"id": "network", "name": "home", "computer": "desk", "role": "owner", "canInvite": true, "canRecover": false,
           "connector": "public",
           "computers": [
             {"name": "desk", "role": "owner", "me": true, "online": true,
              "agents": [{"id": "reviewer", "name": "Reviewer"}], "canRemove": false, "canPromote": false},
             {"name": "laptop", "role": "admin", "me": false, "online": false, "agents": [{"id": "lead", "name": "Lead"}],
              "canRemove": true, "canPromote": false, "later": 1}],
           "rules": [{"fromComputer": "laptop", "fromAgent": "lead", "toAgent": "reviewer", "access": "allow"},
                     {"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "deny"},
                     {"fromComputer": "*", "fromAgent": "*", "toAgent": "coder", "access": "deny"}]},
          {"id": "network-1a2b", "name": "work", "computer": "desk", "role": "member", "computers": [], "rules": []}]}
    """)

    @Test fun `a network view reads every network, its computers, their roles and what they offer`() {
        val read = NetworkView.decode(view)
        assertTrue(read.joined && read.shown && !read.assistants)
        assertEquals(listOf("home", "work"), read.networks.map { it.name })
        val home = read.networks[0]
        assertEquals(listOf("desk", "laptop"), home.computers.map { it.name })
        assertEquals(listOf("Owner", "Admin"), home.computers.map { it.roleTitle })
        assertEquals(listOf("This computer", "Away"), home.computers.map { it.presence })
        assertEquals(listOf(NetworkAgent("reviewer", "Reviewer")), home.computers[0].agents)
    }

    @Test fun `rules read with the bots' names where the network knows them`() {
        val home = NetworkView.decode(view).networks[0]
        val read = home.rules.map { home.describe(it) { template -> template } }
        assertEquals(listOf(
            "Allow: Lead on laptop → Reviewer",
            "Deny: any bot on laptop → any bot here",
            "Deny: any bot on any computer → coder",
        ), read)
        // Each template is translated before the names go in.
        val phrases = mapOf("Deny: {from} → {to}" to "拒絕：{from} → {to}", "any bot on {computer}" to "{computer} 上的任何 Bot",
            "any bot here" to "這裡的任何 Bot")
        assertEquals("拒絕：laptop 上的任何 Bot → 這裡的任何 Bot", home.describe(home.rules[1]) { phrases[it] ?: it })
        // A rule from the command line for one bot on any computer never reads wider than it is.
        assertEquals("Deny: lead on any computer → any bot here", home.describe(NetworkRule("*", "lead", "*", "deny")) { it })
        // Nor does an access this app doesn't know.
        assertEquals("Deny: any bot on any computer → any bot here", home.describe(NetworkRule(access = "ask")) { it })
    }

    @Test fun `a computer in no network shows only when cloud assistants can reach it and are on`() {
        fun alone(on: Boolean, available: Boolean) =
            NetworkView.decode(buildJsonObject { put("joined", false); put("assistants", on); put("assistantsAvailable", available) })
        assertFalse(alone(on = false, available = true).shown)
        assertFalse("not in an account", alone(on = true, available = false).shown)
        assertTrue(alone(on = true, available = true).shown)
    }

    @Test fun `the stream's hello carries the network, and hosts before it none`() {
        fun hello(network: JsonElement?) = MirrorEvent.decode(buildJsonObject {
            put("type", "hello"); put("hostId", "h"); put("rev", 1); network?.let { put("network", it) }
        }) as MirrorEvent.Hello
        assertEquals("desk", hello(view).network?.networks?.first()?.computer)
        assertNull(hello(null).network)
    }

    @Test fun `the network event is live state, and one this app can't read is passed over`() {
        val event = MirrorEvent.decode(buildJsonObject { put("type", "network"); put("network", view) })
        assertEquals("laptop", (event as MirrorEvent.Network).view.networks[0].computers[1].name)
        val broken = buildJsonObject { put("type", "network"); putJsonObject("network") { put("networks", "many") } }
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(broken))
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject { put("type", "network") }))
    }

    @Test fun `the per-bot switches go out only when the host has them`() {
        val old = WireJson.decodeFromJsonElement<Bot>(buildJsonObject { put("id", "a"); put("name", "Old") })
        val new = WireJson.decodeFromJsonElement<Bot>(buildJsonObject {
            put("id", "b"); put("name", "New"); put("offeredToNetwork", true); put("reachableByAssistants", false)
        })
        assertFalse("offeredToNetwork" in BotDraft.from(old).body())
        val body = BotDraft.from(new).copy(offeredToNetwork = false).body()
        assertEquals(JsonPrimitive(false), body["offeredToNetwork"])
        assertEquals(JsonPrimitive(false), body["reachableByAssistants"])
        assertFalse("not part of a template", "offeredToNetwork" in BotDraft.from(new).template())
    }
}
