package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.test.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ConnectionsTest {
    private open class Transport : AppConnectionTransport {
        val foreground = MutableStateFlow(true)
        var starts = 0; var polls = 0; var submissions = 0
        var plan = ComposioConnect("redirect", "https://example.com/sign-in", "connection")
        var answer = ComposioConnectionState("connection", "active")
        override suspend fun awaitActive() { foreground.first { it } }
        override suspend fun start(): ComposioConnect { starts++; return plan }
        override suspend fun state(id: String): ComposioConnectionState { polls++; return answer }
        override suspend fun submit(mode: String, fields: Map<String, String>): ComposioConnectionState { submissions++; return answer }
    }
    private fun rejected(block: () -> Unit) {
        try { block(); fail("Unsupported input must be rejected") } catch (_: IllegalArgumentException) {} catch (_: IllegalStateException) {}
    }
    @Test fun authenticationMetadataRejectsUnsafeLinksAndUnsupportedFields() {
        rejected { ComposioConnect("redirect", "https://user:pass@example.com", "connection").checked() }
        rejected { ComposioConnect("redirect", "javascript:alert(1)", "connection").checked() }
        rejected { ComposioConnect("redirect", "https://example.com", null).checked() }
        rejected { ComposioConnect("future").checked() }
        val field = ComposioField("key", "API key", secret = true, required = true)
        rejected { ComposioConnect("needsFields", mode = "KEY", fields = listOf(field, field)).checked() }
        val plan = ComposioConnect("needsFields", mode = "KEY", fields = listOf(field, ComposioField("optional", "Optional")))
        assertFalse(plan.complete(emptyMap()))
        assertEquals(mapOf("key" to "  fixture secret  "), plan.submitted(mapOf("key" to "  fixture secret  ", "optional" to "")))
        rejected { plan.submitted(mapOf("key" to "fixture", "unknown" to "value")) }
    }
    @Test fun replacementCredentialsPreserveBlankFieldsAndExactWhitespace() {
        val connector = InstalledConnector("fixture", "Fixture", kind = "local", keys = listOf("KEY", "OTHER"))
        assertEquals(mapOf("KEY" to "  fixture value  "), credentialReplacements(connector, mapOf("KEY" to "  fixture value  ", "OTHER" to "")))
        rejected { credentialReplacements(connector, mapOf("unknown" to "fixture")) }
        rejected { credentialReplacements(connector, mapOf("KEY" to "é".repeat(33_000))) }
        assertTrue(ConnectionRequest("connection", "pending", "Fixture", "bot", connectorId = "fixture").canConnect)
        assertFalse(ConnectionRequest("future", "pending", "Fixture", "bot").canConnect)
        assertTrue(ConnectionRequest("agent", "pending", "Claude", "bot", backend = "claude").canConnect)
        assertFalse(ConnectionRequest("agent", "pending", "Claude", "bot").canConnect)
        assertFalse(ConnectionRequest("login", "future", "Fixture", "bot").canConnect)
    }
    @Test fun browserSignInPausesReadsInBackgroundAndReusesItsOriginalConnection() = runTest {
        val transport = Transport()
        val session = AppConnectionSession(backgroundScope, transport)
        runCurrent(); assertEquals(1, transport.starts)
        transport.foreground.value = false
        advanceTimeBy(10_000); runCurrent(); assertEquals(0, transport.polls)
        transport.foreground.value = true
        runCurrent(); assertTrue(session.state.value.connected)
        assertEquals(1, transport.polls); assertEquals(1, transport.starts)
        session.close(); advanceTimeBy(10_000); runCurrent(); assertEquals(1, transport.polls)
    }
    @Test fun unknownStatusesNeverLookConnectedAndRecoverableReadErrorsCanRecover() = runTest {
        val transport = object : Transport() {
            override suspend fun state(id: String): ComposioConnectionState {
                polls++
                if (polls == 1) throw IOException("Temporary read error")
                return ComposioConnectionState(id, if (polls == 2) "future-status" else "active")
            }
        }
        val session = AppConnectionSession(backgroundScope, transport)
        runCurrent(); advanceTimeBy(4_000); runCurrent()
        assertFalse(session.state.value.connected); assertEquals("future-status", session.state.value.status)
        advanceTimeBy(2_000); runCurrent(); assertTrue(session.state.value.connected)
        assertEquals(1, transport.starts); session.close()
    }
    @Test fun anotherConnectionCannotCompleteThisSignInAndPollingHasADeadline() = runTest {
        val wrong = Transport().apply { answer = ComposioConnectionState("other", "active") }
        val session = AppConnectionSession(backgroundScope, wrong)
        runCurrent(); advanceTimeBy(2_000); runCurrent()
        assertFalse(session.state.value.connected); assertNotNull(session.state.value.error); session.close()
        val pending = Transport().apply { answer = ComposioConnectionState("connection", "initiated") }
        val waiting = AppConnectionSession(backgroundScope, pending)
        runCurrent(); advanceTimeBy(600_000); runCurrent()
        assertFalse(waiting.state.value.connected); assertTrue(requireNotNull(waiting.state.value.error).contains("Timed out"))
        assertTrue(pending.polls <= 300); waiting.close()
    }
    @Test fun anUnconfirmedKeySubmissionIsNeverAutomaticallyRetried() = runTest {
        val transport = object : Transport() {
            override suspend fun submit(mode: String, fields: Map<String, String>): ComposioConnectionState {
                submissions++; throw IOException("Uncertain delivery")
            }
        }.apply { plan = ComposioConnect("needsFields", mode = "KEY", fields = listOf(ComposioField("key", "Key", required = true))) }
        val session = AppConnectionSession(backgroundScope, transport)
        runCurrent(); session.submit(mapOf("key" to "fixture")); runCurrent()
        advanceTimeBy(20_000); runCurrent()
        assertEquals(1, transport.submissions); assertEquals(0, transport.polls)
        assertFalse(session.state.value.connected); assertNotNull(session.state.value.error); session.close()
    }
    @Test fun retiringAnAccountCancelsAnInFlightBrowserRead() = runTest {
        val pending = CompletableDeferred<ComposioConnectionState>()
        var cancelled = false
        val transport = object : Transport() {
            override suspend fun state(id: String): ComposioConnectionState {
                try { return pending.await() } finally { cancelled = true }
            }
        }
        val parent = CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler))
        val session = AppConnectionSession(parent, transport)
        runCurrent(); advanceTimeBy(2_000); runCurrent(); parent.cancel(); runCurrent()
        pending.complete(ComposioConnectionState("connection", "active")); runCurrent()
        assertTrue(cancelled); assertFalse(session.state.value.connected)
    }
    @Test fun restoringAnExistingConnectionNeverStartsAnotherSignIn() = runTest {
        val transport = Transport()
        val session = AppConnectionSession(backgroundScope, transport, "connection")
        runCurrent(); assertEquals(0, transport.starts)
        advanceTimeBy(2_000); runCurrent()
        assertTrue(session.state.value.connected)
        assertEquals("connection", session.state.value.connectionId)
        assertEquals(1, transport.polls); assertEquals(0, transport.starts); session.close()
    }
}
