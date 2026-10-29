package com.codync.android

import android.content.Context
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.webrtc.AddIceObserver
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.VideoSink
import org.webrtc.VideoTrack
import java.io.IOException
import java.nio.ByteBuffer
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * One live view of the computer's screen (CodyncUI `ScreenSession`): a WebRTC peer that receives
 * the display as H.264 and sends input over two data channels (`input` reliable, `input-fast`
 * unordered for pointer moves). Signaling negotiates trickle ICE, with complete-SDP fallback
 * for older hosts/helpers. Lives on the main thread; WebRTC callbacks hop onto it.
 */
class ScreenSession(private val store: AppStore, display: ScreenDisplay?) {
    sealed interface Phase {
        data object Connecting : Phase
        data object Live : Phase
        data object Reconnecting : Phase
        data class Failed(val message: String) : Phase
        data object Closed : Phase
    }

    var phase by mutableStateOf<Phase>(Phase.Connecting)
        private set
    /** Text the computer copied, waiting for the user to pull it onto the phone. */
    var remoteClipboard by mutableStateOf<String?>(null)
        private set
    var lastError by mutableStateOf<String?>(null)
        private set
    var display by mutableStateOf(display)
        private set
    /** The display region the video currently shows (points; `null` = all of it), as confirmed by the computer. */
    var streamCrop by mutableStateOf<ScreenRect?>(null)
        private set

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val context: Context = store.getApplication<android.app.Application>()
    private var session: String? = null
    private var pc: PeerConnection? = null
    private var track: VideoTrack? = null
    private var reliable: DataChannel? = null
    private var fast: DataChannel? = null
    private var gathered: CompletableDeferred<Unit>? = null
    private var signaling: ScreenSignaling? = null
    private var recoveryAttempts = 0
    private var recoverJob: Job? = null
    private var renewJob: Job? = null
    private var connectionTimer: Job? = null
    private var generation = 0
    /** Where the video draws; the session moves it between tracks and detaches it before a track is freed. */
    var sink: VideoSink? = null
        set(value) {
            field?.let { track?.removeSink(it) }
            field = value
            value?.let { track?.addSink(it) }
        }

    /** A newer connection or a close replaced the one being set up. */
    private class Superseded : Exception()

    // MARK: connection

    fun start() {
        recoveryAttempts = 0
        phase = Phase.Connecting
        scope.launch {
            try { connect() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Superseded) { /* Its replacement reports for itself. */ }
            catch (error: Exception) {
                if (phase != Phase.Connecting) return@launch
                close()
                phase = Phase.Failed(error.message ?: "Couldn't connect to the remote screen.")
            }
        }
    }

    private suspend fun connect() {
        val preparing = generation
        val active = store.ready()
        val relay = (active.state.value as? LinkState.Ready)?.route == HostRoute.Relay
        val connection = WireJson.decodeFromJsonElement<ScreenConnection>(active.call("screenPrepare", buildJsonObject { put("relay", relay); put("trickle", true) }))
        if (BuildConfig.DEBUG) Log.d("CodyncScreen", "screen prepared: trickle=${connection.trickle}")
        if (generation != preparing || phase == Phase.Closed) {
            closeRemote(connection.session)
            throw Superseded()
        }
        // Reserve the replacement before closing the old viewer so takeover survives renewal.
        val old = session
        teardown()
        val current = generation
        session = connection.session
        if (old != null) closeRemote(old).join()
        if (generation != current || phase == Phase.Closed) throw Superseded()
        val config = PeerConnection.RTCConfiguration(connection.iceServers.map { server ->
            PeerConnection.IceServer.builder(server.urls).apply {
                server.username?.let(::setUsername)
                server.credential?.let(::setPassword)
            }.createIceServer()
        }).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            tcpCandidatePolicy = PeerConnection.TcpCandidatePolicy.ENABLED
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_ONCE
            bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
        }
        val factory = ScreenRtc.factory(context)
        val events = Events(current)
        val pc = factory.createPeerConnection(config, events) ?: throw IOException("Couldn't start the video connection.")
        this.pc = pc
        if (connection.trickle) {
            val next = ScreenSignaling(scope, screenCandidates(active, connection.session), send = { candidate ->
                active.call("screenCandidate", buildJsonObject {
                    put("session", connection.session)
                    put("candidate", WireJson.encodeToJsonElement(candidate))
                })
            }, apply = { candidate ->
                addCandidate(pc, candidate)
            }, isMediaConnected = {
                this.pc === pc && pc.iceConnectionState() in listOf(PeerConnection.IceConnectionState.CONNECTED, PeerConnection.IceConnectionState.COMPLETED)
            }, onFailure = { error ->
                if (generation == current && phase != Phase.Closed) {
                    lastError = error.message
                    teardown()
                    phase = Phase.Reconnecting
                    if (recoverJob?.isActive != true) scheduleRecovery(0)
                }
            })
            signaling = next
            events.signaling = next
            next.subscribe()
            if (this.pc !== pc || generation != current || phase == Phase.Closed) throw Superseded()
            next.check()
        }
        val video = pc.addTransceiver(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO,
            RtpTransceiver.RtpTransceiverInit(RtpTransceiver.RtpTransceiverDirection.RECV_ONLY))
        val codecs = factory.getRtpReceiverCapabilities(MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO).codecs
        video.setCodecPreferences(preferredScreenCodecs(codecs, { it.name }, { it.parameters["profile-level-id"] }))
        track = (video.receiver.track() as? VideoTrack)?.also { t -> sink?.let(t::addSink) }
        reliable = pc.createDataChannel("input", DataChannel.Init())?.also { it.registerObserver(Messages(current)) }
        fast = pc.createDataChannel("input-fast", DataChannel.Init().apply { ordered = false; maxRetransmits = 0 })

        negotiate(pc, active)
        if (generation != current || phase == Phase.Closed) throw Superseded()
        signaling?.check()
        // Refresh both endpoints before TURN credentials expire, including an idle screen.
        renewJob = scope.launch {
            delay(connection.renewIn(System.currentTimeMillis()))
            if (generation != current || phase == Phase.Closed) return@launch
            phase = Phase.Reconnecting
            scheduleRecovery(0)
        }
        connectionTimer = scope.launch {
            delay(30_000)
            if (generation != current || phase == Phase.Live) return@launch
            close()
            phase = Phase.Failed("Couldn't connect to the remote screen. Check the computer's connection and try again.")
        }
    }

    /** Offer → host → answer with the reserved session's ICE configuration. */
    private suspend fun negotiate(pc: PeerConnection, active: EncryptedChannel) {
        val signaling = signaling
        val offer = suspendCancellableCoroutine { done ->
            pc.createOffer(Sdp({ done.resume(it) }, { done.resumeWithException(IOException(it)) }), MediaConstraints())
        }
        if (this.pc !== pc || phase == Phase.Closed) throw Superseded()
        pc.await { pc.setLocalDescription(it, offer) }
        if (this.pc !== pc || phase == Phase.Closed) throw Superseded()
        if (signaling == null) waitForCandidates(pc)
        if (this.pc !== pc || phase == Phase.Closed) throw Superseded()
        val sdp = if (signaling == null) pc.localDescription?.description ?: offer.description else offer.description
        val response = active.call("screenOffer", buildJsonObject {
            put("sdp", sdp)
            session?.let { put("session", it) }
            display?.let { put("display", it.id) }
        }, 30_000)
        val answer = WireJson.decodeFromJsonElement<ScreenAnswer>(response)
        if (this.pc !== pc || phase == Phase.Closed) throw Superseded()
        session = answer.session
        pc.await { pc.setRemoteDescription(it, SessionDescription(SessionDescription.Type.ANSWER, answer.sdp)) }
        if (this.pc !== pc || phase == Phase.Closed) throw Superseded()
        signaling?.installedAnswer()
    }

    private suspend fun addCandidate(pc: PeerConnection, event: ScreenCandidate) = suspendCancellableCoroutine<Unit> { done ->
        // The Linux helper sends no sdpMid; native WebRTC can't take null, and an empty mid falls back to the index.
        val candidate = IceCandidate(event.sdpMid ?: "", requireNotNull(event.sdpMLineIndex), requireNotNull(event.candidate))
        pc.addIceCandidate(candidate, object : AddIceObserver {
            override fun onAddSuccess() { if (done.isActive) done.resume(Unit) }
            override fun onAddFailure(error: String?) {
                if (done.isActive) done.resumeWithException(ScreenProtocolException("Couldn't add a screen ICE candidate."))
            }
        })
    }

    private suspend fun PeerConnection.await(set: (SdpObserver) -> Unit) = suspendCancellableCoroutine { done ->
        set(Sdp(onSet = { done.resume(Unit) }, failed = { done.resumeWithException(IOException(it)) }))
    }

    private suspend fun waitForCandidates(pc: PeerConnection) {
        if (pc.iceGatheringState() == PeerConnection.IceGatheringState.COMPLETE) return
        val done = CompletableDeferred<Unit>()
        gathered = done
        withTimeoutOrNull(10_000) { done.await() }
        if (gathered === done) gathered = null
    }

    private fun iceChanged(state: PeerConnection.IceConnectionState, generation: Int) {
        if (generation != this.generation || phase == Phase.Closed) return
        when (state) {
            PeerConnection.IceConnectionState.CONNECTED, PeerConnection.IceConnectionState.COMPLETED -> {
                signaling?.mediaConnected()
                recoveryAttempts = 0
                connectionTimer?.cancel()
                phase = Phase.Live
                lastError = null
            }
            // Often recovers by itself (Wi-Fi ↔ cellular); reconnect if it does not.
            PeerConnection.IceConnectionState.DISCONNECTED -> { phase = Phase.Reconnecting; scheduleRecovery(2_000) }
            PeerConnection.IceConnectionState.FAILED -> { phase = Phase.Reconnecting; scheduleRecovery(0) }
            else -> Unit
        }
    }

    private fun scheduleRecovery(delayMillis: Long) {
        recoverJob?.cancel()
        recoverJob = scope.launch {
            delay(delayMillis)
            while (isActive && phase == Phase.Reconnecting) {
                if (recoveryAttempts >= 3) {
                    val message = lastError ?: "Couldn't reconnect to the remote screen."
                    close()
                    phase = Phase.Failed(message)
                    return@launch
                }
                // Wait for a usable signaling route before spending a negotiation attempt.
                try { waitForScreenRoute { store.channel?.state?.value } }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) {
                    if (phase != Phase.Reconnecting) return@launch
                    close()
                    phase = Phase.Failed(error.message ?: "Couldn't reach the computer.")
                    return@launch
                }
                try {
                    if (phase != Phase.Reconnecting) return@launch
                    recoveryAttempts += 1
                    connect()
                    return@launch
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Superseded) { /* A failed negotiation may have retired this peer. */ }
                catch (error: Exception) {
                    if (phase == Phase.Closed) return@launch
                    lastError = error.message
                    Log.i("CodyncScreen", "screen reconnect failed: ${error.message}")
                    // The host answers most errors with 400 (a restarting helper, a replaced session): only a denial or a
                    // required update ends recovery early, and the last attempt fails at once, as on the iPhone.
                    if ((error is HostError && error.status in screenTerminalStatuses) || recoveryAttempts >= 3) {
                        close()
                        phase = Phase.Failed(lastError ?: "Couldn't reconnect to the remote screen.")
                        return@launch
                    }
                }
                if (phase != Phase.Reconnecting) return@launch
                delay(minOf(maxOf(1, recoveryAttempts) * 2_000L, 10_000L))
            }
        }
    }

    fun close() {
        recoverJob?.cancel()
        recoverJob = null
        phase = Phase.Closed
        session?.let(::closeRemote)
        session = null
        teardown()
    }

    /** Best effort: a viewer whose close never arrives ends with its WebRTC connection. */
    private fun closeRemote(id: String): Job = scope.launch {
        try { store.hostCall("screenClose", body("session" to id), 15_000) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { Log.i("CodyncScreen", "screen close failed: ${error.message}") }
    }

    private fun teardown() {
        generation += 1
        signaling?.stop()
        signaling = null
        renewJob?.cancel()
        connectionTimer?.cancel()
        gathered?.complete(Unit)
        gathered = null
        sink?.let { track?.removeSink(it) }
        track = null
        reliable?.unregisterObserver()
        for (channel in listOfNotNull(reliable, fast)) { channel.close(); channel.dispose() }
        reliable = null
        fast = null
        pc?.close()
        pc?.dispose()
        pc = null
    }

    // MARK: input

    /** Sends one input event (display points). Pointer moves go unordered: a late move is useless. */
    fun send(event: JsonObject) {
        val move = event["type"]?.jsonPrimitive?.content == "move"
        val channel = (if (move && fast?.state() == DataChannel.State.OPEN) fast else reliable) ?: return
        if (channel.state() != DataChannel.State.OPEN) return
        channel.send(DataChannel.Buffer(ByteBuffer.wrap(event.toString().toByteArray()), false))
    }

    /** What to stream: a region of the display (points; `null` = all of it) at the viewer's pixel size. */
    fun view(crop: ScreenRect?, pixels: ScreenSize) = send(ScreenRequests.view(display?.id, crop, pixels))

    fun switchDisplay(d: ScreenDisplay) {
        display = d
        send(ScreenRequests.switchDisplay(d.id))
    }

    fun pushClipboard(text: String) = send(ScreenRequests.clipboard(text))

    fun takeRemoteClipboard(): String? = remoteClipboard.also { remoteClipboard = null }

    private fun received(reply: ScreenReply) {
        when (reply) {
            is ScreenReply.Clipboard -> remoteClipboard = reply.text
            is ScreenReply.View -> streamCrop = reply.crop
            is ScreenReply.Error -> lastError = reply.message
        }
    }

    // MARK: WebRTC callbacks (its signaling thread) → main

    private inner class Events(private val generation: Int) : PeerConnection.Observer {
        @Volatile var signaling: ScreenSignaling? = null
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) { scope.launch { iceChanged(state, generation) } }
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) {
            if (state != PeerConnection.IceGatheringState.COMPLETE) return
            signaling?.offerLocal(ScreenCandidate.complete)
            scope.launch { if (generation == this@ScreenSession.generation) gathered?.complete(Unit) }
        }
        override fun onSignalingChange(state: PeerConnection.SignalingState) = Unit
        override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
        override fun onIceCandidate(candidate: IceCandidate) {
            signaling?.offerLocal(ScreenCandidate(ScreenCandidate.Kind.Candidate, candidate.sdp, candidate.sdpMLineIndex, candidate.sdpMid))
        }
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
        override fun onAddStream(stream: MediaStream) = Unit
        override fun onRemoveStream(stream: MediaStream) = Unit
        override fun onDataChannel(channel: DataChannel) = Unit
        override fun onRenegotiationNeeded() = Unit
    }

    private inner class Messages(private val generation: Int) : DataChannel.Observer {
        override fun onMessage(buffer: DataChannel.Buffer) {
            // The buffer is only valid during this call.
            val bytes = ByteArray(buffer.data.remaining()).also { buffer.data.get(it) }
            val reply = ScreenReply.decode(bytes.decodeToString()) ?: return
            scope.launch { if (generation == this@ScreenSession.generation) received(reply) }
        }
        override fun onBufferedAmountChange(previous: Long) = Unit
        override fun onStateChange() = Unit
    }

    private class Sdp(val created: (SessionDescription) -> Unit = {}, val failed: (String?) -> Unit, val onSet: () -> Unit = {}) : SdpObserver {
        override fun onCreateSuccess(description: SessionDescription) = created(description)
        override fun onCreateFailure(error: String?) = failed(error)
        override fun onSetSuccess() = onSet()
        override fun onSetFailure(error: String?) = failed(error)
    }
}

/** The process's WebRTC: one EGL context shared by the hardware decoder and the video views. */
internal object ScreenRtc {
    val egl: EglBase by lazy { EglBase.create() }
    private var factory: PeerConnectionFactory? = null

    @Synchronized fun factory(context: Context): PeerConnectionFactory = factory ?: run {
        WebRtc.load(context)
        PeerConnectionFactory.builder()
            .setVideoDecoderFactory(DefaultVideoDecoderFactory(egl.eglBaseContext))
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(egl.eglBaseContext, true, true))
            .createPeerConnectionFactory().also { factory = it }
    }
}
