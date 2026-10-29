package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.*

fun isOpenAILiveModel(model: String): Boolean = model.startsWith("gpt-live-") && !model.contains("transcribe")

/** An additive host capability: old hosts must never receive Live start or validation requests. */
fun requireOpenAILiveSupport(status: JsonObject) {
    if (status["supportsLive"]?.jsonPrimitive?.booleanOrNull != true) {
        throw IOException("Update Codync on the computer to use GPT-Live.")
    }
}

data class LiveCallSession(val sessionId: String, val sdp: String) {
    companion object {
        fun decode(value: JsonObject) = LiveCallSession(
            value.text("sessionId")?.takeIf(String::isNotBlank) ?: throw IOException("The computer sent no Live session ID."),
            value.text("sdp")?.takeIf(String::isNotBlank) ?: throw IOException("OpenAI sent no SDP answer."),
        )
    }
}

/** Host-owned Live protocol and tools. No provider key, transcript assembly, or tool decision lives on the phone. */
interface LiveCallLink {
    suspend fun startLive(model: String, voice: String, sdp: String): LiveCallSession
    suspend fun liveEvent(session: String, event: JsonObject): List<JsonObject>
    suspend fun liveUpdate(session: String, kind: String, text: String): List<JsonObject>
}

/** GPT-Live transports audio over WebRTC and forwards control events to the host in arrival order. */
class OpenAILiveEngine(parent: CoroutineScope, settings: VoiceSettings, private val model: String, private val voice: String,
    private val link: LiveCallLink, private val peer: RealtimePeer,
) : ProviderEngine(parent, VoiceProvider.OpenAI, settings, "OpenAI GPT-Live") {
    private var generation = 0
    private var session: LiveCallSession? = null
    private var connected = false
    private var reconnects = 0
    private var timer: Job? = null
    private var meter: Job? = null
    private var playbackGated = false
    private var closed = CompletableDeferred<Unit>()
    private val pending = ArrayDeque<Pair<String, String>>()

    init { hop { open() } }

    private suspend fun open() {
        val current = ++generation
        closed = CompletableDeferred()
        try {
            val offer = peer.liveOffer(events(current))
            if (!isCurrent(current)) return
            peer.mic(!muted && phase != VoicePhase.Interrupted)
            val started = link.startLive(model, voice, offer)
            if (!isCurrent(current)) return
            session = started
            peer.answer(started.sdp)
            if (!isCurrent(current)) return
            timer?.cancel()
            timer = scope.launch { delay(15_000); hop { if (isCurrent(current) && phase.connecting) lost("Couldn't reach OpenAI GPT-Live.") } }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
            if (!isCurrent(current)) return
            if (connected) lost(error.message.orEmpty()) else fail(error.message ?: "Couldn't start GPT-Live.", VoiceFailureKind.Provider)
        }
    }

    private fun isCurrent(value: Int) = !ended && generation == value

    private fun events(current: Int) = object : RealtimePeer.Events {
        override fun opened() {} // POST /live/sessions already started and configured the session.
        override fun received(text: String) {
            val event = runCatching { WireJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
            // end() releases outside the inbox; let its terminal acknowledgement through during teardown.
            if (generation == current && event.text("type") == "session.closed") closed.complete(Unit)
            hop { if (isCurrent(current)) received(current, event) }
        }
        override fun ice(state: RealtimePeer.Ice) = hop { if (isCurrent(current)) iceChanged(state) }
        override fun closed() = hop { if (isCurrent(current) && connected) lost("Lost the connection to OpenAI GPT-Live.") }
    }

    private suspend fun received(current: Int, event: JsonObject) {
        when (event.text("type")) {
            "session.started" -> {
                if (!phase.connecting) return
                timer?.cancel(); connected = true; reconnects = 0
                phase(VoicePhase.Listening)
                startMeter(current)
                flush(current)
            }
            "session.closed" -> fail("OpenAI GPT-Live ended the session.", VoiceFailureKind.Connection)
            "error", "session.error" -> {
                val detail = (event["error"] as? JsonObject)?.text("message")
                fail(detail ?: "OpenAI GPT-Live rejected a call event.", VoiceFailureKind.Provider)
            }
            "response.event", "session.delegation.created", "session.input_transcript.delta", "session.output_transcript.delta" -> {
                if (event.text("type") == "session.input_transcript.delta" && playbackGated) {
                    playbackGated = false
                    peer.speaker(phase != VoicePhase.Interrupted)
                }
                val id = session?.sessionId ?: return
                val commands = link.liveEvent(id, event)
                if (isCurrent(current)) send(commands)
            }
        }
    }

    private fun startMeter(current: Int) {
        meter?.cancel()
        meter = scope.launch {
            while (isCurrent(current)) {
                val output = withTimeoutOrNull(1_000) { peer.outputLevel() } ?: 0f
                hop {
                    if (isCurrent(current) && active) {
                        phase(if (output > .01f && !playbackGated) VoicePhase.Speaking else VoicePhase.Listening)
                    }
                }
                delay(150)
            }
        }
    }

    private suspend fun iceChanged(state: RealtimePeer.Ice) {
        if (phase.connecting && state != RealtimePeer.Ice.Failed) return
        when (state) {
            RealtimePeer.Ice.Connected -> if (active) timer?.cancel()
            RealtimePeer.Ice.Failed -> lost("Lost the connection to OpenAI GPT-Live.")
            RealtimePeer.Ice.Disconnected -> {
                val current = generation
                timer?.cancel()
                timer = scope.launch { delay(5_000); hop { if (isCurrent(current)) lost("Lost the connection to OpenAI GPT-Live.") } }
            }
        }
    }

    private suspend fun lost(reason: String) {
        if (!connected || reconnects >= Reconnect.delays.size) return fail(reason, VoiceFailureKind.Connection)
        val wait = Reconnect.delays[reconnects++]
        generation++ // Invalidate callbacks before closing the old channel.
        timer?.cancel(); meter?.cancel(); session = null
        if (peer.isOpen) peer.send("{\"type\":\"session.close\"}")
        peer.close()
        playbackGated = false
        phase(VoicePhase.Reconnecting)
        val current = generation
        timer = scope.launch { delay(wait); hop { if (isCurrent(current)) open() } }
    }

    override fun speak(markdown: String): Boolean {
        if (ended) return false
        cue("reply", markdown)
        return true
    }
    override fun announce(notice: String) = cue("notice", notice)
    override fun progress(text: String) = cue("progress", text)
    private fun cue(kind: String, text: String) = hop {
        if (kind == "progress") pending.removeAll { it.first == kind }
        if (pending.size >= 100) throw IOException("Too many voice updates are waiting for GPT-Live.")
        pending.addLast(kind to text)
        flush(generation)
    }

    private suspend fun flush(current: Int) {
        if (!active || !peer.isOpen) return
        val id = session?.sessionId ?: return
        while (pending.isNotEmpty() && isCurrent(current)) {
            val (kind, text) = pending.removeFirst()
            val commands = link.liveUpdate(id, kind, text)
            if (isCurrent(current)) send(commands)
        }
    }

    override fun mute(value: Boolean) = hop {
        mutable.update { it.copy(muted = value, level = if (value) 0f else it.level) }
        peer.mic(!value && phase != VoicePhase.Interrupted)
    }
    override fun focus(active: Boolean) = hop {
        if (!active && this.active) {
            peer.mic(false); peer.speaker(false); phase(VoicePhase.Interrupted)
        } else if (active && phase == VoicePhase.Interrupted) {
            peer.mic(!muted); peer.speaker(!playbackGated); phase(VoicePhase.Listening); flush(generation)
        }
    }
    override fun interrupt() = hop {
        if (phase == VoicePhase.Speaking) {
            // Live handles spoken interruptions continuously; the manual button silences local playback until the user speaks.
            playbackGated = true; peer.speaker(false); phase(VoicePhase.Listening)
        }
    }

    private fun send(events: List<JsonObject>) {
        if (!peer.isOpen) throw IOException("The GPT-Live event channel closed.")
        events.forEach { peer.send(it.toString()) }
    }

    override suspend fun release() {
        peer.mic(false); peer.speaker(false)
        timer?.cancel(); meter?.cancel()
        if (peer.isOpen) {
            peer.send("{\"type\":\"session.close\"}")
            if (withTimeoutOrNull(1_000) { closed.await() } == null) {
                System.err.println("GPT-Live closed locally; provider finalization was not confirmed.")
            }
        }
        generation++; session = null; pending.clear()
        peer.close()
    }
}
