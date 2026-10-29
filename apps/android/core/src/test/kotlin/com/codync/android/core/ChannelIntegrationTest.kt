package com.codync.android.core

import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicReference
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChannelIntegrationTest {
    private val server = MockWebServer()
    private val sockets = CopyOnWriteArrayList<WebSocket>()
    private val hostSeed = ByteArray(32) { 1 }
    private val hostKey = RelayCrypto.signingPublic(hostSeed)
    private val cid = RelayCrypto.computerId(hostKey)
    private val pairBody = AtomicReference<JsonObject>()
    private val cancelled = AtomicReference<Long>()

    private fun computer(): Computer = Computer(cid, "Computer", hostKey.base64Url(), urls = listOf(server.url("/").toString()))
    private fun identity(): DeviceIdentity = DeviceIdentity(ByteArray(32) { 3 }, ByteArray(32) { 12 })

    private fun enqueueHost(forgeSignature: Boolean = false, holdCalls: Boolean = false, burst: Int = 0, welcomeDelay: Long = 0,
        snapshotHead: Int = 5, reject: String? = null, screenBurst: Int = 0) {
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            private lateinit var opener: FrameOpener
            private lateinit var sealer: FrameSealer
            override fun onOpen(webSocket: WebSocket, response: Response) { sockets.add(webSocket) }
            override fun onMessage(webSocket: WebSocket, text: String) {
                val wire = Json.parseToJsonElement(text).jsonObject
                if (wire.text("t") == "hello") {
                    if (reject != null) {
                        webSocket.send(buildJsonObject { put("t", "reject"); put("code", reject); put("message", "Refused: $reject") }.toString())
                        webSocket.close(4001, reject)
                        return
                    }
                    if (welcomeDelay > 0) Thread.sleep(welcomeDelay)
                    val dk = requireNotNull(wire.text("dk")).decodeBase64Url(32)
                    val ek = requireNotNull(wire.text("ek")).decodeBase64Url(32)
                    val nonce = requireNotNull(wire.text("n")).decodeBase64Url(32)
                    assertTrue(RelayCrypto.verify(dk, RelayCrypto.hs1Input(cid.decodeBase64Url(16), dk, ek, nonce),
                        requireNotNull(wire.text("sig")).decodeBase64Url(64)))
                    val hostEphemeral = ByteArray(32) { 5 }
                    val ekH = RelayCrypto.agreementPublic(hostEphemeral)
                    val transcript = RelayCrypto.transcriptHash(cid.decodeBase64Url(16), dk, ek, nonce, ekH)
                    val keys = RelayCrypto.channelKeys(RelayCrypto.sharedSecret(hostEphemeral, ek), transcript)
                    opener = FrameOpener(keys.d2h, RelayCrypto.MAX_OUTBOUND)
                    sealer = FrameSealer(keys.h2d)
                    val signature = RelayCrypto.sign(hostSeed, transcript)
                    if (forgeSignature) signature[0] = (signature[0].toInt() xor 1).toByte()
                    webSocket.send(buildJsonObject { put("t", "welcome"); put("ek", ekH.base64Url()); put("sig", signature.base64Url()) }.toString())
                    return
                }
                if (wire.text("t") != "f") return
                val inner = opener.open(Frame(wire.getValue("c").jsonPrimitive.long, requireNotNull(wire.text("d")).decodeBase64Url())) ?: return
                val request = Json.parseToJsonElement(inner.toString(Charsets.UTF_8)).jsonObject
                val id = request.getValue("id").jsonPrimitive.long
                if (request["cancel"]?.jsonPrimitive?.booleanOrNull == true) {
                    cancelled.set(id)
                    return
                }
                if (holdCalls) return
                if (request.text("sub") == "screenCandidates") {
                    send(webSocket, buildJsonObject { put("id", id); put("ev", buildJsonObject { put("type", "ready") }) })
                    repeat(screenBurst) { index -> send(webSocket, buildJsonObject {
                        put("id", id); put("ev", buildJsonObject {
                            put("type", "candidate"); put("candidate", "candidate:$index 1 udp 1 192.0.2.1 1234 typ host"); put("sdpMLineIndex", 0)
                        })
                    }) }
                } else if (request.text("sub") == "events") {
                    assertEquals("android", request.getValue("b").jsonObject.text("client"))
                    if (burst > 0) {
                        repeat(burst) { index -> send(webSocket, buildJsonObject {
                            put("id", id); put("ev", buildJsonObject { put("type", "future"); put("index", index) })
                        }) }
                        return
                    }
                    send(webSocket, buildJsonObject { put("id", id); put("ev", buildJsonObject { put("type", "hello"); put("rev", snapshotHead) }) })
                    send(webSocket, buildJsonObject { put("id", id); put("ev", buildJsonObject {
                        put("type", "bot"); put("rev", 4); put("bot", buildJsonObject { put("id", "bot-1"); put("name", "Reviewer"); put("rev", 4) })
                    }) })
                } else if (request.text("m") == "pair") {
                    pairBody.set(request.getValue("b").jsonObject)
                    send(webSocket, buildJsonObject { put("id", id); put("ok", buildJsonObject { put("computerId", cid); put("name", "Paired computer") }) })
                    webSocket.close(4100, "paired")
                } else {
                    send(webSocket, buildJsonObject { put("id", id); put("ok", buildJsonObject { put("computerId", cid); put("signKey", hostKey.base64Url()) }) })
                }
            }
            private fun send(socket: WebSocket, inner: JsonObject) {
                sealer.seal(inner.toString().toByteArray()).forEach { frame ->
                    socket.send(buildJsonObject { put("t", "f"); put("c", frame.counter); put("d", frame.data.base64Url()) }.toString())
                }
            }
        }))
    }

    @Test fun `screen overflow cannot block unrelated RPC responses`() = runBlocking {
        enqueueHost(screenBurst = 300)
        val channel = HostConnector.connect(computer(), identity())
        val waiting = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val stream = async {
            runCatching {
                channel.subscribe("screenCandidates", buildJsonObject { put("session", "screen") }).collect { event ->
                    if (event.text("type") == "ready") { waiting.complete(Unit); release.await() }
                }
            }
        }
        try {
            withTimeout(5_000) { waiting.await() }
            val response = withTimeout(5_000) { channel.call("probe") }
            assertEquals(cid, response.jsonObject.text("computerId"))
            release.complete(Unit)
            val result = withTimeout(5_000) { stream.await() }
            assertTrue(result.exceptionOrNull() is ScreenProtocolException)
            withTimeout(5_000) { while (cancelled.get() == null) delay(10) }
        } finally {
            release.complete(Unit)
            stream.cancelAndJoin()
            channel.close()
        }
    }

    @After fun shutdown() {
        sockets.forEach { it.close(1000, null) }
        server.shutdown()
    }

    @Test fun `initial roster uses the current event protocol and releases the subscription`() = runBlocking {
        enqueueHost(snapshotHead = 4)
        val channel = HostConnector.connect(computer(), identity())
        try {
            val bots = HostClient.bots(channel)
            assertEquals(listOf("Reviewer"), bots.map { it.name })
            withTimeout(2_000) { while (cancelled.get() == null) delay(10) }
        } finally { channel.close() }
    }

    @Test fun `pairing sends the Android platform and returns the pinned computer`() = runBlocking {
        enqueueHost()
        val pinned = computer()
        val result = withTimeout(5_000) { HostConnector.pair(Pairing(pinned, ByteArray(16) { 8 }.base64Url()), identity(), "Android phone") }
        assertEquals(cid, result.id)
        assertEquals("Paired computer", result.name)
        assertEquals("android", pairBody.get().text("platform"))
    }

    @Test fun `encrypted RPC and event subscriptions preserve host payloads`() = runBlocking {
        enqueueHost()
        val connected = withTimeout(5_000) { HostConnector.connect(computer(), identity()) }
        try {
            val hello = connected.call("hello")
            assertEquals(cid, hello.jsonObject.text("computerId"))
            val events = withTimeout(5_000) { connected.events().take(2).toList() }
            assertEquals("hello", events.first().text("type"))
            assertEquals("Reviewer", HostClient.decodeBot(events.last())?.name)
        } finally { connected.close() }
    }

    @Test fun `a forged welcome never permits an encrypted request`() = runBlocking {
        enqueueHost(forgeSignature = true)
        var rejected = false
        try { withTimeout(5_000) { HostConnector.connect(computer(), identity()) } }
        catch (_: SecurityException) { rejected = true }
        assertTrue(rejected)
    }

    @Test fun `a lapsed account lease is a transient failure while a revoked device is turned away`() = runBlocking {
        enqueueHost(reject = "leaseExpired")
        enqueueHost(reject = "revoked")
        val lapsed = runCatching { HostConnector.connect(computer(), identity()) }.exceptionOrNull()
        assertTrue(lapsed is IOException && lapsed !is HostError)
        assertEquals("Refused: leaseExpired", lapsed?.message)
        val revoked = runCatching { HostConnector.connect(computer(), identity()) }.exceptionOrNull()
        assertEquals(403, (revoked as HostError).status)
    }

    @Test fun `an uncertain call timeout cancels its response without replaying the mutation`() = runBlocking {
        enqueueHost(holdCalls = true)
        val connected = withTimeout(5_000) { HostConnector.connect(computer(), identity()) }
        try {
            var timedOut = false
            try { connected.call("runRoutine", timeoutMillis = 100) }
            catch (_: IOException) { timedOut = true }
            assertTrue(timedOut)
            assertTrue(currentCoroutineContext().isActive)
            assertTrue(connected.state.value is LinkState.Ready)
            withTimeout(5_000) { while (cancelled.get() == null) kotlinx.coroutines.delay(10) }
            assertEquals(1L, cancelled.get())
        } finally { connected.close() }
    }

    @Test fun `deleted bots use the actual host tombstone format`() {
        val tombstone = Json.parseToJsonElement("""{"type":"bot","rev":6,"bot":{"id":"bot-1","deleted":true,"rev":6}}""").jsonObject
        val bot = requireNotNull(HostClient.decodeBot(tombstone))
        assertTrue(bot.deleted)
        assertEquals(6L, bot.rev)
        assertFails<IllegalArgumentException> {
            HostClient.decodeBot(Json.parseToJsonElement("""{"type":"bot","bot":{"id":"broken"}}""").jsonObject)
        }
    }

    @Test fun `cancelling the owner still cancels an in-flight request`() = runBlocking {
        enqueueHost(holdCalls = true)
        val connected = HostConnector.connect(computer(), identity())
        try {
            var cancelledByOwner = false
            try { withTimeout(100) { connected.call("hello", timeoutMillis = 5_000) } }
            catch (_: kotlinx.coroutines.TimeoutCancellationException) { cancelledByOwner = true }
            assertTrue(cancelledByOwner)
            withTimeout(5_000) { while (cancelled.get() == null) delay(10) }
            assertEquals(1L, cancelled.get())
        } finally { connected.close() }
    }

    @Test fun `a slow consumer receives catch-up larger than both bounded queues without a disconnect`() = runBlocking {
        enqueueHost(burst = 600)
        val connected = HostConnector.connect(computer(), identity())
        try {
            val events = withTimeout(8_000) { connected.events().onEach { delay(2) }.take(600).toList() }
            assertEquals((0 until 600).map(Int::toLong), events.map { it.getValue("index").jsonPrimitive.long })
            assertEquals(cid, connected.call("hello").jsonObject.text("computerId"))
        } finally { connected.close() }
    }

    @Test fun `direct-only waits for a cold handshake beyond the relay fallback budget`() = runBlocking {
        enqueueHost(welcomeDelay = 1800)
        val connected = withTimeout(5_000) { HostConnector.connect(computer().copy(route = ConnectionRoute.DirectOnly), identity()) }
        try { assertEquals(cid, connected.call("hello").jsonObject.text("computerId")) }
        finally { connected.close() }
    }
}
