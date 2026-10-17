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
 * over `oai-events`. The user's key stays on the computer, which mints the client secret.
 */
class OpenAIRealtimeEngine(parent: CoroutineScope, settings: VoiceSettings, private val credential: suspend () -> String,
    private val operator: CallOperator, private val peer: RealtimePeer,
    private val exchange: suspend (secret: String, offer: String) -> String,
) : ProviderEngine(parent, VoiceProvider.OpenAI, settings, "OpenAI Realtime"), RealtimePeer.Events {
    /** A response is being generated; new replies wait for it to finish. */
    private var responding = false
    /** Replies to hand over, and tool results as empty entries (a response is asked for once free). */
    private val pending = mutableListOf<String>()
    private var dropTimer: Job? = null

    init {
        hop {
            val secret = credential()
            if (ended) return@hop
            val offer = peer.offer(this)
            peer.mic(!muted)
            val answer = exchange(secret, offer)
            if (ended) return@hop
            peer.answer(answer)
            drop(15_000, "Couldn't reach OpenAI.") { phase == VoicePhase.Starting }
        }
    }

    override fun speak(markdown: String): Boolean {
        if (ended) return false
        hop { pending += operator.reply(markdown); flush() }
        return true
    }
    override fun announce(notice: String) = hop { pending += operator.notice(notice); flush() }
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
        dropTimer?.cancel()
        send(buildJsonObject {
            put("type", "session.update")
            putJsonObject("session") {
                put("type", "realtime")
                put("instructions", operator.instructions)
                put("tools", JsonArray(CallOperator.tools.map(CallTool::openAI)))
                putJsonObject("audio") { putJsonObject("input") { putJsonObject("turn_detection") { put("type", "semantic_vad") } } }
            }
        })
        phase(VoicePhase.Listening)
        flush()
    }

    override fun received(text: String) = hop {
        val message = runCatching { WireJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return@hop
        when (message.text("type")) {
            "input_audio_buffer.speech_started" -> level(if (muted) 0f else .7f)
            "input_audio_buffer.speech_stopped" -> level(0f)
            "output_audio_buffer.started" -> if (phase == VoicePhase.Listening) phase(VoicePhase.Speaking)
            "output_audio_buffer.stopped", "output_audio_buffer.cleared" -> if (phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
            "response.created" -> responding = true
            "response.done" -> { responding = false; flush() }
            "response.function_call_arguments.done" -> {
                val id = message.text("call_id") ?: return@hop
                val name = message.text("name") ?: return@hop
                val arguments = message.text("arguments")?.let { runCatching { WireJson.parseToJsonElement(it).jsonObject }.getOrNull() }
                val output = operator.call(name, arguments ?: JsonObject(emptyMap()))
                send(buildJsonObject {
                    put("type", "conversation.item.create")
                    putJsonObject("item") { put("type", "function_call_output"); put("call_id", id); put("output", output.toString()) }
                })
                // An empty entry asks for a response once this one is done, so the model speaks the result.
                pending += ""
            }
        }
    }

    override fun ice(state: RealtimePeer.Ice) = hop {
        when (state) {
            RealtimePeer.Ice.Connected -> dropTimer?.cancel()
            RealtimePeer.Ice.Disconnected -> drop(5_000, "Lost the connection to OpenAI.") { true }
            RealtimePeer.Ice.Failed -> fail("Lost the connection to OpenAI.")
        }
    }

    /** Fails the call after `millis` unless something cancels the timer first. */
    private fun drop(millis: Long, message: String, still: () -> Boolean) {
        dropTimer?.cancel()
        dropTimer = scope.launch { delay(millis); hop { if (still()) fail(message) } }
    }

    private fun cancelResponse() {
        send(buildJsonObject { put("type", "response.cancel") })
        send(buildJsonObject { put("type", "output_audio_buffer.clear") })
    }

    /** Hands waiting replies (and tool results, as empty entries) to the model once it's free. */
    private fun flush() {
        if (responding || !active || !peer.isOpen || pending.isEmpty()) return
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
