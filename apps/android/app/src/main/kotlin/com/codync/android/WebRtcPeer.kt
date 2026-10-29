package com.codync.android

import android.content.Context
import com.codync.android.core.RealtimePeer
import java.io.IOException
import java.nio.ByteBuffer
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import org.webrtc.*
import org.webrtc.audio.JavaAudioDeviceModule

/**
 * The WebRTC peer of an OpenAI Realtime call: the microphone as an audio track (hardware echo
 * cancelling on), the model's voice played from the remote track, and the `oai-events` data channel.
 */
internal class WebRtcPeer(private val context: Context) : RealtimePeer {
    private var audio: JavaAudioDeviceModule? = null
    private var factory: PeerConnectionFactory? = null
    private var connection: PeerConnection? = null
    private var source: AudioSource? = null
    private var track: AudioTrack? = null
    private var channel: DataChannel? = null

    override val isOpen: Boolean get() = channel?.state() == DataChannel.State.OPEN

    override suspend fun offer(events: RealtimePeer.Events): String {
        WebRtc.load(context)
        val audio = JavaAudioDeviceModule.builder(context).setAudioAttributes(callAudio)
            .setUseHardwareAcousticEchoCanceler(true).setUseHardwareNoiseSuppressor(true).createAudioDeviceModule()
        this.audio = audio
        val factory = PeerConnectionFactory.builder().setAudioDeviceModule(audio).createPeerConnectionFactory().also { factory = it }
        val config = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN; bundlePolicy = PeerConnection.BundlePolicy.MAXBUNDLE
        }
        val connection = factory.createPeerConnection(config, Observer(events)) ?: throw IOException("Couldn't start the call audio.")
        this.connection = connection
        val source = factory.createAudioSource(MediaConstraints()).also { source = it }
        track = factory.createAudioTrack("mic", source).also { connection.addTrack(it, listOf("call")) }
        channel = connection.createDataChannel("oai-events", DataChannel.Init()).apply {
            registerObserver(object : DataChannel.Observer {
                override fun onBufferedAmountChange(previousAmount: Long) { }
                override fun onStateChange() {
                    when (state()) {
                        DataChannel.State.OPEN -> events.opened()
                        DataChannel.State.CLOSED -> events.closed()
                        else -> { }
                    }
                }
                override fun onMessage(buffer: DataChannel.Buffer) {
                    val bytes = ByteArray(buffer.data.remaining()).also { buffer.data.get(it) }
                    events.received(String(bytes, Charsets.UTF_8))
                }
            })
        }
        val offer = sdp<SessionDescription> { connection.createOffer(it, MediaConstraints()) }
        sdp<Unit> { connection.setLocalDescription(it, offer) }
        return offer.description
    }

    override suspend fun liveOffer(events: RealtimePeer.Events): String {
        offer(events)
        val pc = connection ?: throw IOException("The call ended.")
        withTimeout(10_000) {
            while (pc.iceGatheringState() != PeerConnection.IceGatheringState.COMPLETE) {
                if (connection !== pc) throw IOException("The call ended.")
                delay(50)
            }
        }
        return pc.localDescription?.description ?: throw IOException("The call has no SDP offer.")
    }

    override fun speaker(enabled: Boolean) { audio?.setSpeakerMute(!enabled) }

    override suspend fun outputLevel(): Float = suspendCancellableCoroutine { done ->
        val pc = connection
        if (pc == null) { done.resume(0f); return@suspendCancellableCoroutine }
        pc.getStats { report ->
            val level = report.statsMap.values.filter { it.type == "inbound-rtp" &&
                (it.members["kind"] == "audio" || it.members["mediaType"] == "audio") }
                .mapNotNull { (it.members["audioLevel"] as? Number)?.toFloat() }.maxOrNull() ?: 0f
            if (done.isActive) done.resume(if (connection === pc) level else 0f)
        }
    }

    override suspend fun answer(sdp: String) {
        val connection = connection ?: throw IOException("The call ended.")
        sdp<Unit> { connection.setRemoteDescription(it, SessionDescription(SessionDescription.Type.ANSWER, sdp)) }
    }

    override fun send(text: String) { channel?.send(DataChannel.Buffer(ByteBuffer.wrap(text.toByteArray()), false)) }
    override fun mic(enabled: Boolean) { track?.setEnabled(enabled) }

    override suspend fun close() = withContext(Dispatchers.IO) {
        channel?.unregisterObserver(); channel?.close(); channel?.dispose(); channel = null
        connection?.dispose(); connection = null; track = null
        source?.dispose(); source = null
        factory?.dispose(); factory = null
        audio?.release(); audio = null
    }

    private class Observer(private val events: RealtimePeer.Events) : PeerConnection.Observer {
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) {
            when (state) {
                PeerConnection.IceConnectionState.CONNECTED, PeerConnection.IceConnectionState.COMPLETED -> events.ice(RealtimePeer.Ice.Connected)
                PeerConnection.IceConnectionState.DISCONNECTED -> events.ice(RealtimePeer.Ice.Disconnected)
                PeerConnection.IceConnectionState.FAILED -> events.ice(RealtimePeer.Ice.Failed)
                else -> { }
            }
        }
        override fun onSignalingChange(state: PeerConnection.SignalingState) { }
        override fun onIceConnectionReceivingChange(receiving: Boolean) { }
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) { }
        override fun onIceCandidate(candidate: IceCandidate) { }
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) { }
        override fun onAddStream(stream: MediaStream) { }
        override fun onRemoveStream(stream: MediaStream) { }
        override fun onDataChannel(channel: DataChannel) { }
        override fun onRenegotiationNeeded() { }
    }

    /** One SDP step as a suspending call: `create` results come back as the description, `set` as Unit. */
    @Suppress("UNCHECKED_CAST")
    private suspend fun <T> sdp(step: (SdpObserver) -> Unit): T = suspendCancellableCoroutine { done ->
        step(object : SdpObserver {
            override fun onCreateSuccess(description: SessionDescription) { if (done.isActive) done.resume(description as T) }
            override fun onSetSuccess() { if (done.isActive) done.resume(Unit as T) }
            override fun onCreateFailure(error: String?) { if (done.isActive) done.resumeWithException(IOException("Couldn't start the call audio.")) }
            override fun onSetFailure(error: String?) { if (done.isActive) done.resumeWithException(IOException("OpenAI's answer didn't fit this call.")) }
        })
    }

}

/** Loads WebRTC once per process, for voice calls and the remote screen. */
internal object WebRtc {
    private var loaded = false
    @Synchronized fun load(context: Context) {
        if (loaded) return
        PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(context.applicationContext).createInitializationOptions())
        loaded = true
    }
}
