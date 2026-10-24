package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.*

/** How long a tool call waits for the user's words that are still being written down. */
private const val HEARING_PATIENCE_MS = 2_000L
/** Tries at a call's line while the link comes back. */
private const val END_TRIES = 3

/** The computer keeps a call though it carries no voice model (on-device or speech to text). */
internal fun AppStore.startCallOnHost(call: VoiceBinding, mode: String, provider: VoiceProvider?) {
    contextScope.launch {
        try { hostCall("startCall", buildJsonObject {
            put("botId", call.botId); put("callId", call.id); put("mode", mode); provider?.let { put("provider", it.id) }
            put("language", java.util.Locale.getDefault().toLanguageTag())
        }, 30_000) } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { }
    }
}

/** The call as the computer runs it, made once per call (it outlives an engine switch). */
internal fun VoiceBinding.link(store: AppStore): HostCallLink = link ?: HostCallLink(store, this).also { link = it }

/**
 * [CallLink] over the computer's call methods (`startCall`, `callTool`, `callLines`, `endCall`; iOS `CallLink`). The
 * computer gets what was said, the tool calls and the end one at a time, in order: a hand-off never overtakes the
 * words it hands on, and lines it didn't get go out with the next request.
 */
internal class HostCallLink(private val store: AppStore, private val call: VoiceBinding) : CallLink {
    private val transcript = CallTranscript(script = ::inReadersScript) { flush() }
    private val order = Mutex()
    /** The user's last words are still being written down (OpenAI transcribes them after the turn). */
    private var hearing = false

    override suspend fun start(provider: VoiceProvider, model: String, voice: String): CallSession =
        CallSession.decode(store.hostCall("startCall", buildJsonObject {
            put("botId", call.botId); put("callId", call.id); put("mode", "realtime"); put("provider", provider.id)
            put("model", model); put("voice", voice); put("language", java.util.Locale.getDefault().toLanguageTag())
        }, 30_000).jsonObject)

    override suspend fun tool(name: String, arguments: JsonObject): JsonElement {
        withTimeoutOrNull(HEARING_PATIENCE_MS) { while (hearing) delay(100) }
        return order.withLock {
            val lines = transcript.drain(withOpen = true)
            try {
                store.hostCall("callTool", buildJsonObject {
                    put("callId", call.id); put("name", name); put("arguments", arguments); put("lines", WireJson.encodeToJsonElement(lines))
                }, 60_000).jsonObject["result"] ?: JsonObject(emptyMap())
            } catch (cancelled: CancellationException) { transcript.restore(lines); throw cancelled }
            catch (error: Exception) {
                transcript.restore(lines)
                buildJsonObject { put("error", error.message ?: "The computer didn't answer.") }
            }
        }
    }

    override fun add(from: String, piece: String) {
        if (from == "user") hearing = false
        transcript.add(from, piece)
    }
    override fun transcribing(on: Boolean) { hearing = on }
    override fun endTurn() = transcript.endTurn()
    override fun reply(text: String): String =
        "[${store.state.value.bots.firstOrNull { it.id == call.botId }?.name ?: "Bot"} replied] ${SpokenText.from(text)}"

    /** The call is over: its line in the chat, with what's left of the transcript (the computer ends a call once). */
    suspend fun end(seconds: Long) = order.withLock {
        val lines = WireJson.encodeToJsonElement(transcript.drain(withOpen = true))
        val body = buildJsonObject { put("botId", call.botId); put("callId", call.id); put("seconds", seconds); put("lines", lines) }
        for (attempt in 1..END_TRIES) {
            try { store.hostQuery("endCall", body, 20_000, call.generation); return@withLock }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { if (attempt < END_TRIES) delay(2_000) }
        }
    }

    /** The lines finished so far go to the computer. */
    private fun flush() {
        store.contextScope.launch {
            order.withLock {
                val lines = transcript.drain(withOpen = false)
                if (lines.isEmpty()) return@withLock
                try { store.hostCall("callLines", buildJsonObject { put("callId", call.id); put("lines", WireJson.encodeToJsonElement(lines)) }, 30_000) }
                catch (cancelled: CancellationException) { transcript.restore(lines); throw cancelled }
                catch (_: Exception) { transcript.restore(lines) }
            }
        }
    }
}
