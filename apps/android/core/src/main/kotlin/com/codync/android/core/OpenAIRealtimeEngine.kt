package com.codync.android.core

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.*
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

/** The WebRTC side of an OpenAI Realtime call: the microphone track, the model's voice and the `oai-events` channel. */
interface RealtimePeer {
    enum class Ice { Connected, Disconnected, Failed }
    interface Events { fun opened(); fun received(text: String); fun ice(state: Ice) }
    /** Creates the connection and returns its SDP offer. */
    suspend fun offer(events: Events): String
    suspend fun answer(sdp: String)
    val isOpen: Boolean
    fun send(text: String)
    fun mic(enabled: Boolean)
    suspend fun close()
}

/**
 * OpenAI Realtime over WebRTC, phone ↔ OpenAI directly (iOS `OpenAIRealtimeEngine`): the microphone is
 * a WebRTC audio track, the model's voice plays from the remote track, and events (tools, replies) go
 * over `oai-events`. The computer runs the call ([CallLink]): it mints the client secret on the user's key and writes
 * the voice's instructions and tools. `transcribe` is the model that writes down what the user says.
 */
class OpenAIRealtimeEngine(parent: CoroutineScope, settings: VoiceSettings, private val model: String, private val voice: String,
    private val transcribe: String, private val link: CallLink, private val peer: RealtimePeer,
    private val exchange: suspend (secret: String, offer: String) -> String,
) : ProviderEngine(parent, VoiceProvider.OpenAI, settings, "OpenAI Realtime"), RealtimePeer.Events {
    /** A response is being generated; new replies wait for it to finish. */
    private var responding = false
    /** Replies to hand over, and tool results as empty entries (a response is asked for once free). */
    private val pending = mutableListOf<String>()
    private var dropTimer: Job? = null
    private var session: CallSession? = null
    /** Tool calls still running on the computer: replies wait, so the model gets each result first. */
    private var toolsRunning = 0
    /** The session was set up once: a drop after that is tried again instead of ending the call. */
    private var connected = false
    private var reconnects = 0

    init { hop { open() } }

    /** A session with OpenAI: the computer mints its client secret and writes the voice's instructions (with the call
     * so far, for a session that replaces a dropped one). */
    private suspend fun open() {
        try {
            val session = link.start(VoiceProvider.OpenAI, model, voice)
            if (ended) return
            val secret = session.credential ?: throw IOException("OpenAI sent no session credential.")
            this.session = session
            val offer = peer.offer(this)
            peer.mic(!muted && phase != VoicePhase.Interrupted)
            val answer = exchange(secret, offer)
            if (ended) return
            peer.answer(answer)
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
            // Starting fails over; mid-call, the computer or OpenAI may be back in a moment.
            if (connected) return dropped(error.message.orEmpty()) else throw error
        }
        drop(15_000, "Couldn't reach OpenAI.") { phase.connecting }
    }

    /** The connection to OpenAI dropped mid-call: a new session, a few times, before the call falls back. */
    private suspend fun dropped(reason: String) {
        val delays = Reconnect.delays
        if (reconnects >= delays.size) return fail(reason.ifEmpty { "Lost the connection to OpenAI." })
        val wait = delays[reconnects]
        reconnects++
        dropTimer?.cancel(); responding = false
        peer.close()
        if (phase != VoicePhase.Interrupted) phase(VoicePhase.Reconnecting)
        scope.launch { delay(wait); hop { open() } }
    }

    override fun speak(markdown: String): Boolean {
        if (ended) return false
        hop { pending += link.reply(markdown); flush() }
        return true
    }
    override fun announce(notice: String) = hop { pending += link.notice(notice); flush() }
    /** Only the newest progress waits to be said. */
    override fun progress(text: String) = hop { pending.removeAll { it.startsWith(PROGRESS_TAG) }; pending += link.progress(text); flush() }
    override fun mute(value: Boolean) = hop {
        mutable.update { it.copy(muted = value, level = if (value) 0f else it.level) }
        if (phase != VoicePhase.Interrupted) peer.mic(!value)
    }
    override fun interrupt() = hop { if (phase == VoicePhase.Speaking) { cancelResponse(); phase(VoicePhase.Listening) } }
    override fun focus(active: Boolean) = hop {
        if (!active && this.active) {
            peer.mic(false)
            if (phase == VoicePhase.Speaking) cancelResponse()
            level(0f); phase(VoicePhase.Interrupted)
        } else if (active && phase == VoicePhase.Interrupted) {
            peer.mic(!muted); phase(VoicePhase.Listening); flush()
        }
    }

    override fun opened() = hop {
        send(buildJsonObject {
            put("type", "session.update")
            putJsonObject("session") {
                put("type", "realtime")
                put("instructions", session?.instructions.orEmpty())
                put("tools", JsonArray(session?.tools.orEmpty().map(CallTool::openAI)))
                // What the user says is transcribed too, for the call's line in the chat.
                putJsonObject("audio") { putJsonObject("input") {
                    putJsonObject("turn_detection") { put("type", "semantic_vad") }
                    putJsonObject("transcription") { put("model", transcribe) }
                } }
            }
        })
    }

    override fun received(text: String) = hop {
        val message = runCatching { WireJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return@hop
        when (message.text("type")) {
            // Connected once the session has the instructions and tools, not when the channel opens.
            "session.updated" -> if (phase.connecting) {
                dropTimer?.cancel(); connected = true; reconnects = 0
                phase(VoicePhase.Listening); flush()
            }
            "input_audio_buffer.speech_started" -> level(if (muted) 0f else .7f)
            "input_audio_buffer.speech_stopped" -> level(0f)
            "input_audio_buffer.committed" -> link.transcribing(true)
            "conversation.item.input_audio_transcription.failed" -> link.transcribing(false)
            "output_audio_buffer.started" -> if (phase == VoicePhase.Listening) phase(VoicePhase.Speaking)
            "output_audio_buffer.stopped", "output_audio_buffer.cleared" -> if (phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
            "response.created" -> responding = true
            "response.done" -> { responding = false; flush() }
            "conversation.item.input_audio_transcription.completed" -> { link.add("user", message.text("transcript").orEmpty()); link.endTurn() }
            "response.output_audio_transcript.done" -> { link.add("voice", message.text("transcript").orEmpty()); link.endTurn() }
            "response.function_call_arguments.done" -> {
                val id = message.text("call_id") ?: return@hop
                val name = message.text("name") ?: return@hop
                val arguments = message.text("arguments")?.let { runCatching { WireJson.parseToJsonElement(it).jsonObject }.getOrNull() }
                // The tool runs on the computer, outside the inbox.
                toolsRunning++
                scope.launch {
                    val output = link.tool(name, arguments ?: JsonObject(emptyMap()))
                    hop {
                        toolsRunning--
                        send(buildJsonObject {
                            put("type", "conversation.item.create")
                            putJsonObject("item") { put("type", "function_call_output"); put("call_id", id); put("output", output.toString()) }
                        })
                        // An empty entry asks for a response once this one is done, so the model speaks the result.
                        pending += ""
                        flush()
                    }
                }
            }
        }
    }

    override fun ice(state: RealtimePeer.Ice) = hop {
        // While connecting, only a failure counts: the start timer runs until the session is set up.
        if (phase.connecting && state != RealtimePeer.Ice.Failed) return@hop
        when (state) {
            RealtimePeer.Ice.Connected -> dropTimer?.cancel()
            RealtimePeer.Ice.Disconnected -> drop(5_000, "Lost the connection to OpenAI.") { true }
            RealtimePeer.Ice.Failed -> lost("Lost the connection to OpenAI.")
        }
    }

    /** Gives the session up after `millis` unless something cancels the timer first. */
    private fun drop(millis: Long, message: String, still: () -> Boolean) {
        dropTimer?.cancel()
        dropTimer = scope.launch { delay(millis); hop { if (still()) lost(message) } }
    }

    /** The session is gone: mid-call it's tried again; starting, the call fails over. */
    private suspend fun lost(message: String) {
        if (connected) dropped(message) else fail(message)
    }

    private fun cancelResponse() {
        send(buildJsonObject { put("type", "response.cancel") })
        send(buildJsonObject { put("type", "output_audio_buffer.clear") })
    }

    /** Hands waiting replies (and tool results, as empty entries) to the model once it's free. */
    private fun flush() {
        if (responding || toolsRunning > 0 || !active || !peer.isOpen || pending.isEmpty()) return
        for (text in pending) if (text.isNotEmpty()) send(buildJsonObject {
            put("type", "conversation.item.create")
            putJsonObject("item") {
                put("type", "message"); put("role", "user")
                putJsonArray("content") { addJsonObject { put("type", "input_text"); put("text", text) } }
            }
        })
        pending.clear()
        responding = true
        send(buildJsonObject { put("type", "response.create") })
    }

    private fun send(event: JsonObject) { if (peer.isOpen) peer.send(event.toString()) }

    override suspend fun release() { dropTimer?.cancel(); peer.close() }


    companion object {
        const val CALLS = "https://api.openai.com/v1/realtime/calls"

        /** Posts the offer with the call's client secret and returns OpenAI's SDP answer. */
        suspend fun answer(http: OkHttpClient, secret: String, offer: String, url: String = CALLS): String = withContext(Dispatchers.IO) {
            val request = Request.Builder().url(url).header("Authorization", "Bearer $secret")
                .post(offer.toByteArray().toRequestBody("application/sdp".toMediaType())).build()
            http.newBuilder().callTimeout(15, TimeUnit.SECONDS).build().newCall(request).execute().use { response ->
                val body = response.body?.string().orEmpty()
                if (response.isSuccessful) return@use body
                val message = runCatching { WireJson.parseToJsonElement(body).jsonObject["error"]?.jsonObject?.text("message") }.getOrNull()
                throw IOException(message ?: "OpenAI answered ${response.code}.")
            }
        }
    }
}
