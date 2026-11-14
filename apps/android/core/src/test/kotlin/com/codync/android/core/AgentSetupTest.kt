package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.test.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AgentSetupTest {
    private class Sink : TerminalSink {
        var text = ""; var resets = 0; var enabled = false
        override suspend fun write(bytes: ByteArray) { text += bytes.toString(Charsets.UTF_8) }
        override fun reset() { resets++; text = "" }
        override fun input(enabled: Boolean) { this.enabled = enabled }
    }
    private open class Transport : SetupTerminalTransport {
        val start = CompletableDeferred<String>()
        val closed = mutableListOf<String>()
        val inputs = mutableListOf<String>()
        val sizes = mutableListOf<Pair<Int, Int>>()
        override suspend fun setup(cols: Int, rows: Int) = start.await()
        override fun output(term: String) = flow<TerminalEvent> { awaitCancellation() }
        override suspend fun input(term: String, bytes: ByteArray) { inputs.add(bytes.toString(Charsets.UTF_8)) }
        override suspend fun resize(term: String, cols: Int, rows: Int) { sizes.add(cols to rows) }
        override suspend fun close(term: String) { closed.add(term) }
    }
    @Test fun backBeforeTheSetupReplyClosesTheLateTerminalExactlyOnce() = runTest {
        val transport = Transport()
        val session = SetupTerminalSession(backgroundScope, transport)
        runCurrent(); session.close()
        assertTrue(transport.closed.isEmpty())
        transport.start.complete("late-session"); runCurrent()
        assertEquals(listOf("late-session"), transport.closed)
        session.closeAndJoin()
        assertEquals(listOf("late-session"), transport.closed)
    }
    @Test fun terminalInputIsOrderedAndAFullQueueStopsFurtherInput() = runTest {
        val gate = CompletableDeferred<Unit>()
        val transport = object : Transport() {
            override suspend fun input(term: String, bytes: ByteArray) { super.input(term, bytes); gate.await() }
        }
        val session = SetupTerminalSession(backgroundScope, transport)
        val sink = Sink(); session.attach(sink)
        transport.start.complete("session"); runCurrent()
        session.type("first".toByteArray()); runCurrent()
        session.type("second".toByteArray()); runCurrent()
        assertEquals(listOf("first"), transport.inputs)
        gate.complete(Unit); runCurrent()
        assertEquals(listOf("first", "second"), transport.inputs)
        session.resized(90, 30); runCurrent()
        assertEquals(listOf(90 to 30), transport.sizes)
        // Saturation occurs before the sender can run; never silently lose a keystroke and continue.
        repeat(66) { session.type("x".toByteArray()) }
        assertTrue(requireNotNull(session.state.value.error).contains("queue"))
        assertFalse(sink.enabled)
        runCurrent()
        assertEquals(listOf("first", "second"), transport.inputs)
        session.closeAndJoin()
    }
    @Test fun reconnectResetsScrollbackAndReplaysTheExistingTerminal() = runTest {
        var subscriptions = 0
        val transport = object : Transport() {
            override fun output(term: String) = flow {
                subscriptions++
                emit(TerminalEvent.Output("head".toByteArray()))
                if (subscriptions == 1) { emit(TerminalEvent.Output("live".toByteArray())); throw IOException("Drop") }
                emit(TerminalEvent.Exit(0))
            }
        }
        val session = SetupTerminalSession(backgroundScope, transport)
        val sink = Sink(); session.attach(sink); transport.start.complete("session"); runCurrent()
        assertEquals("headlive", sink.text)
        advanceTimeBy(1_000); runCurrent()
        assertEquals("head", sink.text); assertEquals(2, sink.resets)
        assertEquals(0, session.state.value.exitCode); assertFalse(sink.enabled)
        session.type("after exit".toByteArray()); runCurrent(); assertTrue(transport.inputs.isEmpty())
        val replacement = Sink(); session.detach(sink); session.attach(replacement); runCurrent()
        assertEquals("head", replacement.text)
        assertEquals(3, subscriptions)
        session.closeAndJoin(); assertEquals(listOf("session"), transport.closed)
    }
    @Test fun anUnconfirmedInputIsNeverAutomaticallyRetried() = runTest {
        val transport = object : Transport() {
            override suspend fun input(term: String, bytes: ByteArray) { super.input(term, bytes); throw IOException("Reply lost") }
        }
        val session = SetupTerminalSession(backgroundScope, transport)
        val sink = Sink(); session.attach(sink); transport.start.complete("session"); runCurrent()
        session.type("maybe sent".toByteArray()); runCurrent()
        session.type("later".toByteArray()); advanceTimeBy(5_000); runCurrent()
        assertEquals(listOf("maybe sent"), transport.inputs)
        assertFalse(sink.enabled); assertTrue(requireNotNull(session.state.value.error).contains("wasn't confirmed"))
        session.closeAndJoin()
    }
    @Test fun authKeysRespectSavedNamesAndUnknownMethodsRemainAvailableForAnUpgrade() {
        val auth = WireJson.decodeFromString<AgentAuth>("""{"signedIn":false,"methods":[{"id":"keys","name":"API key","kind":"envVar","vars":[{"name":"KEY","label":"Key","secret":true,"optional":false}]},{"id":"future","name":"New method","kind":"newKind"}],"savedEnv":["KEY"]}""")
        val method = auth.methods.first()
        assertTrue(method.complete(emptyMap(), auth.savedEnv))
        assertFalse(method.complete(emptyMap(), emptyList()))
        assertTrue(method.complete(mapOf("KEY" to "fixture-value"), emptyList()))
        assertEquals("newKind", auth.methods.last().kind)
        val output = TerminalEvent.decode(buildJsonObject { put("type", "output"); put("data", "aGk=") })
        assertArrayEquals("hi".toByteArray(), (output as TerminalEvent.Output).bytes)
        try { TerminalEvent.decode(buildJsonObject { put("type", "output"); put("data", "!") }); fail("Reject malformed bytes") } catch (_: IllegalArgumentException) {}
    }
}
