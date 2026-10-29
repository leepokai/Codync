package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class MarketTest {
    @Test fun catalogPagesDeduplicate() {
        val page = CatalogPage<String>().append((1..25).map(Int::toString)) { it }
        assertEquals(25, page.items.size)
        val next = page.append(listOf("2", "26", "26")) { it }
        assertEquals(26, next.items.size)
    }

    @Test fun connectorInputsRetainHostDefaultsAndUnknownAuthorizationDoesNotClaimSignIn() {
        val input = WireJson.decodeFromString<MarketInput>("""{"name":"REGION","required":true,"default":"provided-by-host","future":true}""")
        val option = InstallOption("local", "npm", "On computer", listOf(input))
        assertTrue(option.complete(emptyMap()))
        assertFalse(option.complete(mapOf("REGION" to "")))
        assertTrue(InstalledConnector("c", "Connector", kind = "remote", auth = "signedOut").needsSignIn)
        assertFalse(InstalledConnector("c", "Connector", kind = "remote", auth = "future").needsSignIn)
    }
}
