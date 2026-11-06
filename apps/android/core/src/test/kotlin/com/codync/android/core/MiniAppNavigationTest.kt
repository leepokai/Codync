package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class MiniAppNavigationTest {
    private val address = MiniAppAddress("bot", "tracker", "owner")

    @Test fun onlyTheExactHttpsVirtualOriginStaysInsideTheWebView() {
        assertEquals(MiniAppNavigation.InApp, miniAppNavigation(address, address.pageUrl))
        assertEquals(MiniAppNavigation.InApp, miniAppNavigation(address, address.pageUrl.replace(".invalid/", ".invalid:443/")))
        for (url in listOf("https://example.com/", "https://codync-app.invalid.evil/", "http://codync-app.invalid/",
            "https://codync-app.invalid:8443/")) {
            assertEquals(url, MiniAppNavigation.Browser, miniAppNavigation(address, url))
        }
        for (url in listOf("javascript:alert(1)", "file:///tmp/page.html", "content://example/page", "data:text/html,hello",
            "intent://example", "https://user@codync-app.invalid/", "https://codync-app.invalid/apps/bot/other/owner/")) {
            assertEquals(url, MiniAppNavigation.Blocked, miniAppNavigation(address, url))
        }
    }

    @Test fun bridgeRejectsSubframesLookalikesAndOtherCapabilities() {
        val gate = MiniAppPageGate(address)
        assertNull(gate.admit(MINI_APP_ORIGIN, true, address.pageUrl))
        gate.started(address.pageUrl)
        assertNotNull(gate.admit(MINI_APP_ORIGIN, true, address.pageUrl))
        assertNull(gate.admit(MINI_APP_ORIGIN, false, address.pageUrl))
        assertNull(gate.admit("https://codync-app.invalid.evil", true, address.pageUrl))
        assertNull(gate.admit("http://codync-app.invalid", true, address.pageUrl))
        assertNull(gate.admit(MINI_APP_ORIGIN, true, address.pageUrl.replace("owner", "other")))
        assertNull(gate.admit(MINI_APP_ORIGIN, true, address.pageUrl + "%2e%2e/other"))
        assertNull(gate.admit("null", true, address.pageUrl))
    }

    @Test fun navigationInvalidatesRepliesEvenWhenTheNewDocumentHasTheSameUrl() {
        val gate = MiniAppPageGate(address)
        gate.started(address.pageUrl)
        val ticket = gate.admit(MINI_APP_ORIGIN, true, address.pageUrl)!!
        assertTrue(gate.mayReply(ticket, address.pageUrl))
        assertFalse(gate.mayReply(ticket, "https://example.com/"))
        gate.started(address.pageUrl)
        assertFalse(gate.mayReply(ticket, address.pageUrl))
        gate.started("https://example.com/")
        assertNull(gate.admit(MINI_APP_ORIGIN, true, address.pageUrl))
    }
}
