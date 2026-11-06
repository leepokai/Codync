package com.codync.android.core

import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

enum class HostRoute { Direct, Relay }
sealed interface LinkState {
    data object Connecting : LinkState
    data class Ready(val route: HostRoute) : LinkState
    data class ComputerOffline(val lastSeenAt: Long?) : LinkState
    data class Failed(val message: String, val status: Int? = null) : LinkState
}

class HostError(val status: Int, override val message: String) : IOException(message)

/** One encrypted connection; its owner decides when to reconnect and when to retire it. */
class EncryptedChannel internal constructor(
    private val socket: ChannelSocket,
    val route: HostRoute,
    private val computer: Computer,
    private val identity: DeviceIdentity,
    private val pairing: Boolean,
) : AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val writer = Mutex()
    private val mutableState = MutableStateFlow<LinkState>(LinkState.Connecting)
    val state: StateFlow<LinkState> = mutableState.asStateFlow()
    private val firstState = CompletableDeferred<Unit>()
    private var handshake: Handshake? = null
    private var handshakeDeadline: Job? = null
    private var sealer: FrameSealer? = null
    private var opener: FrameOpener? = null
    private val ids = AtomicLong()
    private val calls = ConcurrentHashMap<Long, CompletableDeferred<JsonElement>>()
    private data class Subscription(val kind: String, val events: Channel<JsonObject>)
    private val streams = ConcurrentHashMap<Long, Subscription>()
    private val mailboxCalls = ConcurrentHashMap<String, CompletableDeferred<JsonObject>>()
    private val listLock = Mutex()
    private val mailboxUpdates = MutableSharedFlow<JsonObject>(extraBufferCapacity = 256)
    val mailbox = mailboxUpdates.asSharedFlow()
    @Volatile private var closed = false

    internal suspend fun start(): EncryptedChannel {
        scope.launch {
            try {
                if (route == HostRoute.Direct) beginHandshake()
                while (true) {
                    val text = if (route == HostRoute.Relay) withTimeout(75_000) { socket.receive() } else socket.receive()
                    val wire = Json.parseToJsonElement(text).jsonObject
                    handle(wire)
                }
            } catch (error: CancellationException) {
                if (!closed) fail(IOException("Connection to the computer timed out.", error))
            } catch (error: Throwable) {
                val failure = if (error is ChannelClosed) when (error.code) {
                    4001, 4003, 4410 -> HostError(403, requireNotNull(error.message))
                    4400 -> HostError(426, requireNotNull(error.message))
                    else -> error
                } else error
                fail(failure)
            }
        }
        if (route == HostRoute.Relay) scope.launch {
            try {
                while (true) {
                    delay(30_000)
                    // The relay's auto-response requires these exact bytes.
                    writer.withLock { socket.send("{\"t\":\"ping\"}") }
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                fail(error)
            }
        }
        try {
            deadline(15_000, "The computer didn't finish connecting.") { firstState.await() }
            return this
        } catch (error: Throwable) {
            close()
            throw error
        }
    }

    private suspend fun beginHandshake() {
        handshakeDeadline?.cancel()
        mutableState.value = LinkState.Connecting
        dropPending(IOException("The computer connection changed. Retry the action."))
        writer.withLock {
            sealer = null
            opener = null
            val next = Handshake(computer, identity)
            handshake = next
            mutableState.value = LinkState.Connecting
            socket.send(next.hello(pairing).toString())
        }
        handshakeDeadline = scope.launch {
            delay(10_000)
            fail(IOException("The computer didn't finish connecting."))
        }
    }

    private suspend fun handle(wire: JsonObject) {
        when (wire.text("t")) {
            "presence" -> if (wire["online"]?.jsonPrimitive?.booleanOrNull == true) {
                beginHandshake()
            } else {
                handshakeDeadline?.cancel()
                writer.withLock { handshake = null; sealer = null; opener = null }
                dropPending(IOException("The computer is offline."))
                mutableState.value = LinkState.ComputerOffline(wire["lastSeenAt"]?.jsonPrimitive?.longOrNull)
                firstState.complete(Unit)
            }
            "welcome" -> {
                val current = requireNotNull(handshake) { "Unexpected host welcome" }
                val keys = current.finish(requireNotNull(wire.text("ek")).decodeBase64Url(32),
                    requireNotNull(wire.text("sig")).decodeBase64Url(64))
                writer.withLock {
                    handshake = null
                    sealer = FrameSealer(keys.d2h)
                    opener = FrameOpener(keys.h2d)
                }
                handshakeDeadline?.cancel()
                mutableState.value = LinkState.Ready(route)
                firstState.complete(Unit)
            }
            "reject" -> {
                val message = wire.text("message")
                throw when (wire.text("code")) {
                    // The computer couldn't confirm the account's approval lately; it may again, so the owner retries.
                    "leaseExpired" -> IOException(message ?: "The computer can't confirm this device's access right now. Try again shortly.")
                    "unsupportedVersion" -> HostError(426, message ?: "Update Codync to connect to this computer.")
                    else -> HostError(403, message ?: "This device isn't allowed on the computer. Pair it again.")
                }
            }
            "f" -> {
                val current = requireNotNull(opener) { "Encrypted frame arrived before the handshake" }
                val counter = requireNotNull(wire["c"]?.jsonPrimitive?.longOrNull)
                val data = requireNotNull(wire.text("d")).decodeBase64Url()
                val message = current.open(Frame(counter, data))
                if (message != null) handleInner(Json.parseToJsonElement(message.toString(Charsets.UTF_8)).jsonObject)
            }
            "pong" -> Unit
            "mbox.ok", "mbox.err", "mbox.cancelled", "mbox.gone", "mbox.items" -> {
                val key = if (wire.text("t") == "mbox.items") "list" else requireNotNull(wire.text("nonce"))
                mailboxCalls.remove(key)?.complete(wire)
            }
            "mbox.delivered", "mbox.failed", "mbox.expired" -> {
                if (!mailboxUpdates.tryEmit(wire)) throw IOException("Mailbox updates fell behind. Reconnect to reconcile.")
            }
            else -> Unit
        }
    }

    private suspend fun handleInner(message: JsonObject) {
        val id = requireNotNull(message["id"]?.jsonPrimitive?.longOrNull) { "Missing RPC ID" }
        require(id in 1..0xffff_ffffL) { "Invalid RPC ID" }
        when {
            "ev" in message -> {
                val subscription = streams[id] ?: return
                val events = subscription.events
                if (subscription.kind == "screenCandidates") {
                    val result = events.trySend(message.getValue("ev").jsonObject)
                    if (result.isFailure && !result.isClosed && streams.remove(id, subscription)) {
                        events.close(ScreenProtocolException("Screen ICE candidate queue overflowed."))
                        cancelRemote(id)
                    }
                    return
                }
                // Bounded suspension reaches the socket reader instead of dropping catch-up.
                // A closed subscription is expected when the user leaves that screen.
                try { events.send(message.getValue("ev").jsonObject) }
                catch (_: kotlinx.coroutines.channels.ClosedSendChannelException) { }
                catch (error: CancellationException) { val task = kotlinx.coroutines.currentCoroutineContext(); task.ensureActive() }
            }
            message["end"]?.jsonPrimitive?.booleanOrNull == true -> streams.remove(id)?.events?.close()
            "err" in message -> {
                val detail = message.getValue("err").jsonObject
                val failure = HostError(detail["status"]?.jsonPrimitive?.longOrNull?.toInt() ?: 500,
                    detail.text("message") ?: "The computer could not complete the request.")
                calls.remove(id)?.completeExceptionally(failure)
                streams.remove(id)?.events?.close(failure)
            }
            "ok" in message -> calls.remove(id)?.complete(message.getValue("ok"))
            else -> throw IOException("Invalid RPC response")
        }
    }

    suspend fun awaitReady(timeoutMillis: Long = 15_000) {
        val current = deadline(timeoutMillis, "The computer didn't finish connecting.") { state.first { it !is LinkState.Connecting } }
        when (current) {
            is LinkState.Ready -> Unit
            is LinkState.ComputerOffline -> throw IOException("The computer is offline.")
            is LinkState.Failed -> if (current.status != null) throw HostError(current.status, current.message) else throw IOException(current.message)
            LinkState.Connecting -> error("Unreachable")
        }
    }

    suspend fun call(method: String, body: JsonElement = JsonNull, timeoutMillis: Long = 20_000): JsonElement {
        awaitReady(timeoutMillis)
        require(!pairing || method == "pair") { "A pairing channel only accepts pair" }
        val id = nextId()
        val result = CompletableDeferred<JsonElement>()
        calls[id] = result
        try {
            val request = buildJsonObject { put("id", id); put("m", method); put("b", body) }
            sendInner(request)
            val response = deadline(timeoutMillis, "The computer didn't respond in time. Check the result before retrying.") { result.await() }
            return response
        } finally {
            if (calls.remove(id) != null) cancelRemote(id)
        }
    }

    fun events(since: Long = 0): Flow<JsonObject> = subscribe("events", buildJsonObject { put("since", since); put("client", "android") })

    fun subscribe(kind: String, body: JsonObject): Flow<JsonObject> = flow {
        awaitReady()
        require(!pairing)
        val id = nextId()
        val events = Channel<JsonObject>(if (kind == "screenCandidates") ScreenCandidate.maxCount + 2 else 256)
        streams[id] = Subscription(kind, events)
        try {
            sendInner(buildJsonObject {
                put("id", id); put("sub", kind); put("b", body)
            })
            for (event in events) emit(event)
        } finally {
            val subscribed = streams.remove(id) != null
            events.cancel()
            if (subscribed) cancelRemote(id)
        }
    }

    suspend fun enqueue(send: PendingSend) {
        require(route == HostRoute.Relay && !pairing && send.files.isEmpty()) { "Only text can wait in the relay mailbox." }
        val inner = buildJsonObject {
            put("m", "send"); put("ts", System.currentTimeMillis())
            put("b", send.body())
        }
        val blob = RelayCrypto.sealMailbox(inner.toString().toByteArray(), computer, identity, send.nonce)
        val result = mailboxRequest(send.nonce, buildJsonObject { put("t", "mbox.put"); put("nonce", send.nonce); put("d", blob) })
        if (result.text("t") == "mbox.err") throw IOException(result.text("code") ?: "The relay couldn't queue this message.")
    }

    suspend fun queued(): JsonObject = listLock.withLock {
        mailboxRequest("list", buildJsonObject { put("t", "mbox.list") })
    }

    suspend fun cancelQueued(nonce: String): String {
        val result = mailboxRequest(nonce, buildJsonObject { put("t", "mbox.cancel"); put("nonce", nonce) })
        return if (result.text("t") == "mbox.cancelled") "cancelled" else result.text("state") ?: "unknown"
    }

    private suspend fun mailboxRequest(key: String, message: JsonObject): JsonObject {
        require(route == HostRoute.Relay && !pairing)
        val deferred = CompletableDeferred<JsonObject>()
        check(mailboxCalls.putIfAbsent(key, deferred) == null) { "This mailbox action is already pending." }
        try {
            writer.withLock { check(!closed); socket.send(message.toString()) }
            val response = deadline(15_000, "The relay didn't respond in time. Reconnect to check delivery.") { deferred.await() }
            return response
        } finally { mailboxCalls.remove(key, deferred) }
    }

    private fun nextId(): Long {
        val id = ids.incrementAndGet()
        require(id <= 0xffff_ffffL) { "Channel must be replaced before request IDs wrap" }
        return id
    }

    private suspend fun sendInner(message: JsonObject) {
        writer.withLock {
            if (closed) throw IOException("Connection to the computer ended.")
            val current = sealer ?: throw IOException("The computer isn't connected.")
            val frames = current.seal(message.toString().toByteArray())
            try {
                for (frame in frames) socket.send(buildJsonObject {
                    put("t", "f"); put("c", frame.counter); put("d", frame.data.base64Url())
                }.toString())
            } catch (error: Throwable) {
                // A failed writer consumed counters. This channel can no longer be reused.
                fail(error)
                throw error
            }
        }
    }

    private fun cancelRemote(id: Long) {
        scope.launch {
            try { sendInner(buildJsonObject { put("id", id); put("cancel", true) }) }
            catch (error: CancellationException) { throw error }
            catch (_: Exception) { /* The owner will reconnect; an uncertain mutation is never replayed here. */ }
        }
    }

    private fun dropPending(error: Throwable) {
        calls.keys.toList().forEach { calls.remove(it)?.completeExceptionally(error) }
        streams.keys.toList().forEach { streams.remove(it)?.events?.close(error) }
    }

    private fun fail(error: Throwable) {
        if (closed) return
        mutableState.value = LinkState.Failed(error.message ?: "Can't reach the computer.", (error as? HostError)?.status)
        firstState.completeExceptionally(error)
        dropPending(error)
        close()
    }

    override fun close() {
        if (closed) return
        closed = true
        if (mutableState.value !is LinkState.Failed) mutableState.value = LinkState.Failed("Connection closed.")
        firstState.cancel()
        dropPending(IOException("Connection closed."))
        mailboxCalls.keys.toList().forEach { mailboxCalls.remove(it)?.completeExceptionally(IOException("Relay connection closed.")) }
        socket.close()
        scope.cancel()
    }
}

internal fun JsonObject.text(key: String): String? = this[key]?.takeUnless { it == JsonNull }?.jsonPrimitive?.content
