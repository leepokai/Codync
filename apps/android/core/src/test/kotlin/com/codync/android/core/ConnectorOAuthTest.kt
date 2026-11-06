package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class ConnectorOAuthTest {
    @Test fun aPendingStateHashAcceptsItsCallbackOnlyOnTheOriginalComputerWithinItsDeadline() {
        val plan = ConnectorSignIn("https://provider.example/authorize?state=fixture-state", "app")
        val pending = PendingConnectorOAuth.create(plan, "connector", "computer", 1000)
        assertFalse(pending.toString().contains("fixture-state"))
        val back = OAuthReturn.parse("codync://oauth?state=fixture-state&code=fixture-code", "codync")
        pending.validate(back, "computer", 2000)
        for ((callback, computer, now) in listOf(Triple(back.copy(state = "other"), "computer", 2000L),
            Triple(back, "other-computer", 2000L), Triple(back, "computer", pending.expiresAt))) {
            try { pending.validate(callback, computer, now); fail("Wrong or expired callback must be rejected") }
            catch (_: IllegalArgumentException) { }
        }
    }
    @Test fun callbackParsingRejectsRepeatedFieldsMixedResultsAndOtherDestinations() {
        val cancelled = OAuthReturn.parse("codync://oauth?state=fixture&error=access_denied&error_description=User+cancelled", "codync")
        assertEquals("User cancelled", cancelled.error)
        assertNull(cancelled.code)
        for (value in listOf("codync://bot/id?state=s&code=c", "codync://oauth/path?state=s&code=c",
            "codync://oauth?state=s&state=x&code=c", "codync://oauth?state=s&code=c&code=x", "codync://oauth?state=s&code=c&error=bad",
            "codync://oauth?state=s&code=c#fragment", "codync://oauth?code=c")) {
            try { OAuthReturn.parse(value, "codync"); fail("Malformed return must be rejected") } catch (_: IllegalArgumentException) { }
        }
        try { PendingConnectorOAuth.create(ConnectorSignIn("http://provider.example?state=s", "app"), "c", "host", 1000); fail() }
        catch (_: IllegalArgumentException) { }
    }
}
