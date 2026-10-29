package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.decodeFromJsonElement
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ScreenSignalingTest {
    private fun candidate(index: Int) = ScreenCandidate(ScreenCandidate.Kind.Candidate,
        "candidate:$index 1 udp 1 192.0.2.1 1234 typ host", 0, "video")

    private class Fixture(scope: CoroutineScope) {
        val incoming = Channel<ScreenCandidate>(Channel.UNLIMITED)
        val uploads = mutableListOf<ScreenCandidate>()
        val applied = mutableListOf<ScreenCandidate>()
        val failures = mutableListOf<Throwable>()
        var connected = false
        var uploadGate: CompletableDeferred<Unit>? = null
        var applyGate: CompletableDeferred<Unit>? = null
        val signal = ScreenSignaling(scope, incoming.receiveAsFlow(), send = {
            uploadGate?.await()
            uploads += it
        }, apply = {
            applyGate?.await()
            applied += it
        }, isMediaConnected = { connected }, onFailure = { failures += it })
        fun ready() { incoming.trySend(ScreenCandidate(ScreenCandidate.Kind.Ready)).getOrThrow() }
    }

    @Test fun `older hosts keep complete SDP unless they explicitly negotiate trickle`() {
        val old = WireJson.parseToJsonElement("""{"session":"s","iceServers":[],"expiresAt":1}""")
        assertFalse(WireJson.decodeFromJsonElement<ScreenConnection>(old).trickle)
        val newer = WireJson.parseToJsonElement("""{"session":"s","iceServers":[],"expiresAt":1,"trickle":true}""")
        assertTrue(WireJson.decodeFromJsonElement<ScreenConnection>(newer).trickle)
    }

    @Test fun `candidates are bounded by UTF8 bytes and reject malformed fields`() {
        candidate(1).validate()
        ScreenCandidate.complete.validate()
        val invalid = listOf(candidate(1).copy(candidate = "candidate:x\n"), candidate(1).copy(candidate = "no prefix"),
            candidate(1).copy(candidate = "candidate:" + "é".repeat(2048)),
            candidate(1).copy(sdpMid = "é".repeat(129)), candidate(1).copy(sdpMLineIndex = -1),
            candidate(1).copy(sdpMLineIndex = 17), candidate(1).copy(sdpMLineIndex = null))
        for (event in invalid) assertThrows(ScreenProtocolException::class.java) { event.validate() }
    }

    @Test fun `both directions wait for answer then preserve ordering and completion acknowledgments`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.signal.offerLocal(candidate(1)); f.signal.offerLocal(candidate(2)); f.signal.offerLocal(ScreenCandidate.complete)
        f.incoming.send(candidate(3)); f.incoming.send(candidate(4)); f.incoming.send(ScreenCandidate.complete)
        runCurrent()
        assertTrue(f.uploads.isEmpty()); assertTrue(f.applied.isEmpty())
        val upload = CompletableDeferred<Unit>(); f.uploadGate = upload
        val apply = CompletableDeferred<Unit>(); f.applyGate = apply
        f.signal.installedAnswer(); runCurrent()
        assertTrue(f.uploads.isEmpty()); assertTrue(f.applied.isEmpty()); assertFalse(f.signal.candidatesComplete)
        apply.complete(Unit); runCurrent()
        assertEquals(listOf(candidate(3), candidate(4)), f.applied)
        assertFalse(f.signal.candidatesComplete)
        upload.complete(Unit); runCurrent()
        assertEquals(listOf(candidate(1), candidate(2), ScreenCandidate.complete), f.uploads)
        assertTrue(f.signal.candidatesComplete); assertTrue(f.failures.isEmpty())
        f.signal.stop()
    }

    @Test fun `a signaling failure before ready throws its reason`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { runCatching { f.signal.subscribe() } }
        runCurrent()
        f.incoming.trySend(ScreenCandidate(ScreenCandidate.Kind.Error, message = "unknown screen session")).getOrThrow()
        runCurrent()
        val error = subscription.await().exceptionOrNull()
        assertTrue(error is ScreenProtocolException)
        assertEquals("unknown screen session", error?.message)
    }

    @Test fun `losing signaling before media and completion fails negotiation`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.incoming.close(IOException("socket closed")); runCurrent()
        assertEquals(1, f.failures.size); assertFalse(f.signal.isDetached)
        assertThrows(IOException::class.java) { f.signal.check() }
    }

    @Test fun `native connected state preserves media even before delegate callback arrives`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.connected = true
        f.incoming.close(IOException("socket closed")); runCurrent()
        assertTrue(f.signal.isDetached); assertTrue(f.failures.isEmpty()); f.signal.check()
        f.signal.offerLocal(candidate(1)); runCurrent()
        assertTrue(f.uploads.isEmpty())
    }

    @Test fun `completed directions preserve peer when signaling ends before connected callback`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.signal.installedAnswer()
        f.signal.offerLocal(ScreenCandidate.complete); f.incoming.send(ScreenCandidate.complete); runCurrent()
        assertTrue(f.signal.candidatesComplete)
        f.incoming.close(); runCurrent()
        assertTrue(f.signal.isDetached); assertTrue(f.failures.isEmpty())
    }

    @Test fun `unacknowledged completion does not detach an unconnected negotiation`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.uploadGate = CompletableDeferred()
        f.signal.installedAnswer()
        f.signal.offerLocal(ScreenCandidate.complete); f.incoming.send(ScreenCandidate.complete); runCurrent()
        assertFalse(f.signal.candidatesComplete)
        f.incoming.close(IOException("socket closed")); runCurrent()
        assertEquals(1, f.failures.size); assertFalse(f.signal.isDetached)
    }

    @Test fun `protocol errors stay fatal after media connects`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        f.connected = true; f.signal.installedAnswer()
        f.incoming.send(ScreenCandidate.complete); f.incoming.send(candidate(1)); runCurrent()
        assertTrue(f.failures.single() is ScreenProtocolException); assertFalse(f.signal.isDetached)
    }

    @Test fun `native candidate rejection stays fatal on a connected peer`() = runTest {
        val incoming = Channel<ScreenCandidate>(4)
        val failures = mutableListOf<Throwable>()
        val signal = ScreenSignaling(backgroundScope, incoming.receiveAsFlow(), {},
            { throw IOException("native failure") }, { true }, { failures += it })
        val subscription = async { signal.subscribe() }
        runCurrent(); incoming.send(ScreenCandidate(ScreenCandidate.Kind.Ready)); runCurrent(); subscription.await()
        signal.installedAnswer(); incoming.send(candidate(1)); runCurrent()
        assertTrue(failures.single() is ScreenProtocolException); assertFalse(signal.isDetached)
    }

    @Test fun `host rejection stays fatal on a connected peer`() = runTest {
        val incoming = Channel<ScreenCandidate>(4)
        val failures = mutableListOf<Throwable>()
        val signal = ScreenSignaling(backgroundScope, incoming.receiveAsFlow(),
            { throw HostError(500, "Helper rejected the candidate.") }, {}, { true }, { failures += it })
        val subscription = async { signal.subscribe() }
        runCurrent(); incoming.send(ScreenCandidate(ScreenCandidate.Kind.Ready)); runCurrent(); subscription.await()
        signal.installedAnswer(); signal.offerLocal(candidate(1)); runCurrent()
        assertTrue(failures.single() is HostError); assertFalse(signal.isDetached)
    }

    @Test fun `local overflow fails before offer can silently lose candidates`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await()
        repeat(10_000) { f.signal.offerLocal(candidate(it)) }; runCurrent()
        assertTrue(f.failures.single() is ScreenProtocolException); assertTrue(f.uploads.isEmpty())
    }

    @Test fun `remote count limit rejects the candidate beyond the bound`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.ready(); runCurrent(); subscription.await(); f.signal.installedAnswer()
        repeat(ScreenCandidate.maxCount + 1) { f.incoming.send(candidate(it)) }; runCurrent()
        assertEquals(ScreenCandidate.maxCount, f.applied.size)
        assertTrue(f.failures.single() is ScreenProtocolException)
    }

    @Test fun `stopping an unopened subscription unblocks readiness without failure`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { f.signal.subscribe() }
        runCurrent(); f.signal.stop(); runCurrent()
        assertTrue(subscription.isCancelled); assertTrue(f.failures.isEmpty())
    }

    @Test fun `subscription readiness has a finite deadline`() = runTest {
        val f = Fixture(backgroundScope)
        val subscription = async { runCatching { f.signal.subscribe() } }
        runCurrent(); advanceTimeBy(10_000); runCurrent()
        val result = subscription.await()
        assertTrue(result.isFailure); assertEquals(1, f.failures.size)
    }
}
