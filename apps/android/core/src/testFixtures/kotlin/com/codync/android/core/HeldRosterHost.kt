package com.codync.android.core

import kotlinx.serialization.json.*
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer

/** A direct host whose first, finite roster subscription stays unanswered until the test releases it. */
class HeldRosterHost : AutoCloseable {
    private val server = MockWebServer().apply { start() }
    private val hostSeed = ByteArray(32) { 1 }
    private val hostKey = RelayCrypto.signingPublic(hostSeed)
    private val computerId = RelayCrypto.computerId(hostKey)
    private val lock = Object()
    private var opener: FrameOpener? = null
    private var sealer: FrameSealer? = null
    private var held: Pair<WebSocket, Long>? = null
    @Volatile var rosterHeld = false
        private set
    @Volatile var eventSubscriptions = 0
        private set

    val computer = Computer(computerId, "Computer", hostKey.base64Url(), urls = listOf(server.url("/").toString()))

    init {
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) = synchronized(lock) {
                val wire = Json.parseToJsonElement(text).jsonObject
                if (wire.text("t") == "hello") {
                    val deviceKey = requireNotNull(wire.text("dk")).decodeBase64Url(32)
                    val ephemeral = requireNotNull(wire.text("ek")).decodeBase64Url(32)
                    val nonce = requireNotNull(wire.text("n")).decodeBase64Url(32)
                    val hostEphemeral = ByteArray(32) { 5 }
                    val hostPublic = RelayCrypto.agreementPublic(hostEphemeral)
                    val transcript = RelayCrypto.transcriptHash(computerId.decodeBase64Url(16), deviceKey, ephemeral, nonce, hostPublic)
                    val keys = RelayCrypto.channelKeys(RelayCrypto.sharedSecret(hostEphemeral, ephemeral), transcript)
                    opener = FrameOpener(keys.d2h, RelayCrypto.MAX_OUTBOUND)
                    sealer = FrameSealer(keys.h2d)
                    webSocket.send(buildJsonObject {
                        put("t", "welcome"); put("ek", hostPublic.base64Url()); put("sig", RelayCrypto.sign(hostSeed, transcript).base64Url())
                    }.toString())
                    return@synchronized
                }
                if (wire.text("t") != "f") return@synchronized
                val frame = Frame(wire.getValue("c").jsonPrimitive.long, requireNotNull(wire.text("d")).decodeBase64Url())
                val inner = opener?.open(frame) ?: return@synchronized
                val request = Json.parseToJsonElement(inner.toString(Charsets.UTF_8)).jsonObject
                if (request["cancel"]?.jsonPrimitive?.booleanOrNull == true) return@synchronized
                val id = request.getValue("id").jsonPrimitive.long
                when {
                    request.text("sub") == "events" -> {
                        eventSubscriptions += 1
                        if (eventSubscriptions == 1) {
                            held = webSocket to id
                            rosterHeld = true
                        } else sendSnapshot(webSocket, id)
                    }
                    request.text("sub") != null -> Unit
                    else -> send(webSocket, buildJsonObject {
                        put("id", id)
                        put("ok", buildJsonObject {
                            put("computerId", computerId); put("signKey", hostKey.base64Url()); put("name", "Computer"); put("version", "99.0.0")
                            put("hostId", computerId)
                        })
                    })
                }
            }
        }))
    }

    fun releaseRoster() = synchronized(lock) {
        val (socket, id) = requireNotNull(held)
        held = null
        rosterHeld = false
        sendSnapshot(socket, id)
    }

    private fun sendSnapshot(socket: WebSocket, id: Long) {
        send(socket, buildJsonObject { put("id", id); put("ev", buildJsonObject { put("type", "hello"); put("hostId", computerId); put("rev", 2); put("catchUp", 2) }) })
        for (index in 1..2) send(socket, buildJsonObject {
            put("id", id)
            put("ev", buildJsonObject {
                put("type", "bot"); put("rev", index)
                put("bot", buildJsonObject { put("id", "bot-$index"); put("name", "Bot $index"); put("rev", index) })
            })
        })
    }

    private fun send(socket: WebSocket, inner: JsonObject) {
        requireNotNull(sealer).seal(inner.toString().toByteArray()).forEach { frame ->
            socket.send(buildJsonObject { put("t", "f"); put("c", frame.counter); put("d", frame.data.base64Url()) }.toString())
        }
    }

    override fun close() {
        server.shutdown()
    }
}
