package com.codync.android.core

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
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
    @Test fun aSignInWithACodeCarriesTheCode() {
        val plan = WireJson.decodeFromString<ConnectorSignIn>("""{"url":"https://github.com/login/device","callback":"host","code":"ABCD-1234"}""")
        assertEquals("ABCD-1234", plan.code)
        assertNull(WireJson.decodeFromString<ConnectorSignIn>("""{"url":"https://a.example","callback":"app"}""").code)
    }
    @Test fun copyAndOpenAnswersYesWhicheverCloseComesFirstAndCancelAnswersNo() = runBlocking {
        val prompt = SignInCodeAnswer()
        val later = mutableListOf<() -> Unit>()
        fun ask() = async(start = CoroutineStart.UNDISPATCHED) { prompt.ask() }
        fun nextTurn() { later.forEach { it() }; later.clear() }
        // The dialog closes, then Copy and open acts: the close's no comes a turn later.
        var answer = ask()
        prompt.dismissed { later += it }
        prompt.open()
        nextTurn()
        assertTrue(answer.await())
        // Copy and open, then the dialog closes.
        answer = ask()
        prompt.open()
        prompt.dismissed { later += it }
        nextTurn()
        assertTrue(answer.await())
        // Cancel.
        answer = ask()
        prompt.dismissed { later += it }
        nextTurn()
        assertFalse(answer.await())
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
