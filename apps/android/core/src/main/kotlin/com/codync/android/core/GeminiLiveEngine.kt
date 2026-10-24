package com.codync.android.core

import java.net.URLEncoder
import java.util.Base64
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.*
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString

/**
 * Gemini Live over a WebSocket, phone ↔ Google directly (iOS `GeminiLiveEngine`): 16 kHz PCM up,
 * 24 kHz PCM down, echo-cancelled so you can talk over it. The computer runs the call ([CallLink]): it mints one-use
 * ephemeral tokens on the user's key and writes the voice's instructions and tools, again for each session; a session
 * the server ends (`goAway`) is resumed with a fresh one.
 */
class GeminiLiveEngine(parent: CoroutineScope, settings: VoiceSettings, private val model: String, private val voice: String,
    private val link: CallLink,
    private val mic: MicSource, private val player: PcmPlayer, private val http: OkHttpClient,
    private val endpoint: String = ENDPOINT,
) : ProviderEngine(parent, VoiceProvider.Gemini, settings, "Gemini Live") {
    private var socket: WebSocket? = null
    /** Setup is done on the current socket. */
    private var ready = false
    private var resumeHandle: String? = null
    private var reconnects = 0
    private var audioRunning = false
    /** Chunks handed to the player and not heard yet; bumping the generation drops late callbacks. */
    private var queued = 0
    private var playGeneration = 0
    /** The model finished its turn (nothing more is coming until we or the user speak). */
    private var turnDone = true
    /** After a local interrupt, the rest of the turn's audio is dropped. */
    private var dropping = false
    private val pending = mutableListOf<String>()
    /** The current provider session's instructions and tools, from the computer. */
    private var session: CallSession? = null
    /** Tool calls still running on the computer: replies wait, so the model gets each answer first. */
    private var toolsRunning = 0

    init { hop { connect() } }

    override fun speak(markdown: String): Boolean {
        if (ended) return false
        hop { pending += link.reply(markdown); flush() }
        return true
    }
    override fun announce(notice: String) = hop { pending += link.notice(notice); flush() }
    override fun mute(value: Boolean) = hop {
        if (value == muted) return@hop
        mutable.update { it.copy(muted = value, level = 0f) }
        if (value) send(buildJsonObject { putJsonObject("realtimeInput") { put("audioStreamEnd", true) } })
    }
    override fun interrupt() = hop {
        if (phase != VoicePhase.Speaking) return@hop
        stopPlayback(); dropping = !turnDone; phase(VoicePhase.Listening)
    }
    override fun focus(active: Boolean) = hop {
        if (!active && this.active) {
            stopPlayback(); dropping = !turnDone; level(0f); phase(VoicePhase.Interrupted)
            send(buildJsonObject { putJsonObject("realtimeInput") { put("audioStreamEnd", true) } })
        } else if (active && phase == VoicePhase.Interrupted) { phase(VoicePhase.Listening); flush() }
    }

    private suspend fun connect() {
        val session = link.start(VoiceProvider.Gemini, model, voice)
        if (ended) return
        val token = session.credential ?: throw IllegalStateException("Gemini sent no session credential.")
        this.session = session
        ready = false
        val url = "$endpoint?access_token=" + URLEncoder.encode(token, "UTF-8")
        socket = http.newWebSocket(Request.Builder().url(url).build(), Listener())
    }

    private inner class Listener : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) = hop { if (socket === webSocket) send(setup(), webSocket) }
        override fun onMessage(webSocket: WebSocket, text: String) = hop { if (socket === webSocket) received(text) }
        override fun onMessage(webSocket: WebSocket, bytes: ByteString) = hop { if (socket === webSocket) received(bytes.utf8()) }
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(1000, null); hop { closed(webSocket, reason) } }
        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = hop { closed(webSocket, reason) }
        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = hop { closed(webSocket, "") }
    }

    private suspend fun closed(webSocket: WebSocket, reason: String) {
        if (socket !== webSocket) return
        socket = null; ready = false
        if (resumeHandle != null && reconnects < 3) { reconnects++; connect() }
        else fail(reason.ifEmpty { "Lost the connection to Gemini." })
    }

    private fun setup() = buildJsonObject {
        putJsonObject("setup") {
            put("model", "models/$model")
            putJsonObject("generationConfig") {
                putJsonArray("responseModalities") { add("AUDIO") }
                putJsonObject("speechConfig") { putJsonObject("voiceConfig") { putJsonObject("prebuiltVoiceConfig") { put("voiceName", voice) } } }
            }
            putJsonObject("systemInstruction") { putJsonArray("parts") { addJsonObject { put("text", session?.instructions.orEmpty()) } } }
            putJsonArray("tools") { addJsonObject { put("functionDeclarations", JsonArray(session?.tools.orEmpty().map(CallTool::gemini))) } }
            // Audio sessions are otherwise capped at 15 minutes.
            putJsonObject("contextWindowCompression") { putJsonObject("slidingWindow") { } }
            // What both sides said, for the call's line in the chat.
            putJsonObject("inputAudioTranscription") { }
            putJsonObject("outputAudioTranscription") { }
            putJsonObject("sessionResumption") { resumeHandle?.let { put("handle", it) } }
        }
    }

    private suspend fun received(text: String) {
        val message = runCatching { WireJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
        if ("setupComplete" in message) {
            ready = true; reconnects = 0
            if (!audioRunning) {
                mic.start({ pcm, level -> hop { captured(pcm, level) } }) {
                    hop { fail("Couldn't use the microphone. Check its permission and other audio apps.") }
                }
                audioRunning = true
            }
            flush()
        }
        message.objectAt("serverContent")?.let { content ->
            content.objectAt("inputTranscription")?.text("text")?.let { link.add("user", it) }
            content.objectAt("outputTranscription")?.text("text")?.let { link.add("voice", it) }
            if (content.flag("turnComplete") || content.flag("interrupted")) link.endTurn()
            if (content.flag("interrupted")) {
                stopPlayback(); dropping = false
                if (phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
            }
            (content.objectAt("modelTurn")?.get("parts") as? JsonArray)?.let { parts ->
                turnDone = false
                for (part in parts) (part as? JsonObject)?.objectAt("inlineData")?.text("data")?.let { if (!dropping) play(it) }
            }
            if (content.flag("turnComplete")) {
                turnDone = true; dropping = false
                if (queued == 0 && phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
                flush()
            }
        }
        (message.objectAt("toolCall")?.get("functionCalls") as? JsonArray)?.let { calls ->
            val asked = calls.mapNotNull { call ->
                val fields = call as? JsonObject ?: return@mapNotNull null
                Triple(fields.text("id") ?: return@mapNotNull null, fields.text("name") ?: return@mapNotNull null,
                    fields.objectAt("args") ?: JsonObject(emptyMap()))
            }
            // The tools run on the computer, outside the inbox; their answers go back together.
            toolsRunning++
            scope.launch {
                val responses = asked.map { (id, name, args) ->
                    val result = link.tool(name, args)
                    buildJsonObject { put("id", id); put("name", name); putJsonObject("response") { put("result", result) } }
                }
                hop {
                    toolsRunning--
                    send(buildJsonObject { putJsonObject("toolResponse") { put("functionResponses", JsonArray(responses)) } })
                    flush()
                }
            }
        }
        message.objectAt("sessionResumptionUpdate")?.let { update ->
            if (update.flag("resumable")) update.text("newHandle")?.let { resumeHandle = it }
        }
        // The server ends this connection soon; carry on in a resumed one.
        if ("goAway" in message) socket?.cancel()
    }

    private fun send(message: JsonObject, on: WebSocket? = null) {
        val target = on ?: socket?.takeIf { ready } ?: return
        target.send(message.toString())
    }

    /** Replies wait for the model to finish talking; `turnComplete` here would cut it off. */
    private fun flush() {
        if (!ready || !turnDone || queued != 0 || toolsRunning > 0 || pending.isEmpty() || phase == VoicePhase.Interrupted) return
        val text = pending.joinToString("\n\n"); pending.clear()
        turnDone = false
        send(buildJsonObject {
            putJsonObject("clientContent") {
                putJsonArray("turns") { addJsonObject { put("role", "user"); putJsonArray("parts") { addJsonObject { put("text", text) } } } }
                put("turnComplete", true)
            }
        })
    }

    private fun captured(pcm: ByteArray, level: Float) {
        // Connected once the microphone delivers after setup, not when the socket opens.
        if (ready && phase == VoicePhase.Starting) phase(VoicePhase.Listening)
        if (muted || !active) return
        level(if (phase == VoicePhase.Speaking) 0f else level)
        send(buildJsonObject {
            putJsonObject("realtimeInput") {
                putJsonObject("audio") { put("data", Base64.getEncoder().encodeToString(pcm)); put("mimeType", "audio/pcm;rate=16000") }
            }
        })
    }

    private fun play(base64: String) {
        val pcm = runCatching { Base64.getDecoder().decode(base64) }.getOrNull()?.takeIf { it.size >= 2 } ?: return
        queued++
        level(0f); phase(VoicePhase.Speaking)
        val generation = playGeneration
        player.play(pcm) { hop { played(generation) } }
    }

    private fun played(generation: Int) {
        if (generation != playGeneration) return
        queued = maxOf(0, queued - 1)
        if (queued != 0 || !turnDone) return
        if (phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
        flush()
    }

    private fun stopPlayback() { playGeneration++; queued = 0; player.stop() }

    override suspend fun release() {
        val open = socket; socket = null; ready = false
        open?.close(1000, null)
        stopPlayback()
        if (audioRunning) { audioRunning = false; mic.stop() }
        player.close()
    }

    private fun JsonObject.objectAt(key: String): JsonObject? = this[key] as? JsonObject
    private fun JsonObject.flag(key: String): Boolean = (this[key] as? JsonPrimitive)?.booleanOrNull == true

    companion object {
        const val ENDPOINT = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContentConstrained"
    }
}
