package com.codync.android.core

import java.net.URLEncoder
import org.junit.Assert.assertEquals
import org.junit.Test

class PairingTest {
    private val fixture = vectors()
    private val keys = fixture.getValue("keys").let { it as kotlinx.serialization.json.JsonObject }
    private val params = linkedMapOf(
        "v" to "3", "id" to requireNotNull(keys.text("computerId")), "name" to "Fixture computer",
        "sk" to requireNotNull(keys.text("hostSignPub")), "bk" to requireNotNull(keys.text("hostBoxPub")),
        "code" to ByteArray(16) { 8 }.base64Url(), "urls" to "http://192.168.1.2:19222,https://example.test:19222",
        "cloud" to "https://cloud.example.test",
    )
    private fun link(fields: Map<String, String> = params): String = "codync://pair?" + fields.entries.joinToString("&") {
        "${it.key}=${URLEncoder.encode(it.value, "UTF-8")}" }

    @Test fun `a valid pairing preserves pins direct candidates relay and name`() {
        val pairing = Pairing.parse(" ${link()} \n", "codync")
        assertEquals(params["id"], pairing.computer.id)
        assertEquals(params["name"], pairing.computer.name)
        assertEquals(2, pairing.computer.urls.size)
        assertEquals(params["cloud"], pairing.computer.cloud)
        assertEquals(fixture.getValue("pairing").let { it as kotlinx.serialization.json.JsonObject }.text("offerId"), pairing.offerId)
    }

    @Test fun `pairing again keeps the color route and push switch picked on this phone`() {
        val paired = Pairing.parse(link(), "codync").computer
        val picked = paired.copy(name = "Old name", color = "mint", route = ConnectionRoute.DirectOnly, notifications = false)
        val kept = paired.keepingDevicePicks(picked)
        assertEquals(Triple("mint", ConnectionRoute.DirectOnly, false), Triple(kept.color, kept.route, kept.notifications))
        assertEquals(params["name"], kept.name)
        assertEquals(paired, paired.keepingDevicePicks(null))
    }

    @Test fun `Codync Dev and Codync only accept their own pairing links`() {
        val dev = link().replace("codync://", "codync-dev://")
        assertEquals(params["id"], Pairing.parse(dev, "codync-dev").computer.id)
        assertFails<IllegalArgumentException> { Pairing.parse(dev, "codync") }
        assertFails<IllegalArgumentException> { Pairing.parse(link(), "codync-dev") }
    }

    @Test fun `altered pins damaged keys obsolete links and duplicate fields are rejected`() {
        for (fields in listOf(params + ("id" to "invalid"), params + ("sk" to "invalid"), params + ("bk" to "invalid"),
            params + ("code" to "invalid"), params + ("v" to "2"), params + ("cloud" to "http://example.test"),
            params + ("cloud" to "https://user:password@example.test"))) {
            assertFails<IllegalArgumentException> { Pairing.parse(link(fields), "codync") }
        }
        assertFails<IllegalArgumentException> { Pairing.parse(link() + "&sk=" + params["sk"], "codync") }
        assertFails<IllegalArgumentException> { Pairing.parse("https://example.test/?v=3", "codync") }
    }

    @Test fun `a rejected link says why`() {
        fun kind(text: String) = assertFails<PairingProblem> { Pairing.parse(text, "codync") }.kind
        assertEquals(PairingProblem.Kind.NotPairingLink, kind("https://example.test/?v=3"))
        assertEquals(PairingProblem.Kind.OutdatedHost, kind(link(params + ("v" to "2"))))
        assertEquals(PairingProblem.Kind.Invalid, kind(link(params + ("sk" to "invalid"))))
    }

    @Test fun `unsafe direct endpoints are removed and at least one route must remain`() {
        val pairing = Pairing.parse(link(params + ("urls" to "javascript:alert(1),http://user:pass@host.test")), "codync")
        assertEquals(emptyList<String>(), pairing.computer.urls)
        assertFails<IllegalArgumentException> { Pairing.parse(link(params + ("urls" to "ftp://host.test") - "cloud"), "codync") }
    }

    @Test fun `strict base64url refuses padding noncanonical bits and wrong lengths`() {
        for (value in listOf("AA=", "AB", "A", "+A", "AA ")) assertFails<IllegalArgumentException> { value.decodeBase64Url() }
        assertFails<IllegalArgumentException> { "AA".decodeBase64Url(32) }
    }
}

