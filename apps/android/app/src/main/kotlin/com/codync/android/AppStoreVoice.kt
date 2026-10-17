package com.codync.android

import java.util.UUID
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// A voice call with one bot: what you say is sent, its replies are read aloud.

/** The call in progress and what it needs: its session, start time and which replies it has read. */
internal data class VoiceBinding(val id: String, val botId: String, val generation: Int,
    val startedAt: Long, val session: VoiceSession, val replies: VoiceReplyGate)
fun AppStore.startVoice(botId: String, expectedGeneration: Int = state.value.generation) {
    if (state.value.generation != expectedGeneration || state.value.loading || state.value.voice != null) return
    if (!foreground || state.value.actualConnection !is LinkState.Ready || state.value.bots.none { it.id == botId && !it.deleted && it.kind != "group" }) {
        mutableState.update { it.copy(error = "Open the connected bot's conversation to start voice chat.") }; return
    }
    val id = UUID.randomUUID().toString()
    voiceGeneration = expectedGeneration
    mutableState.update { it.copy(voice = VoiceCallViewState(id, botId, VoiceState(settings = VoicePreferences.load(getApplication())))) }
    try { VoiceService.start(getApplication(), this, id, botId); PushDelivery.foreground = true }
    catch (_: Exception) { finishVoice(id, "Couldn't start voice chat. Allow microphone access and wait for any previous call to finish.") }
}
internal fun AppStore.attachVoice(id: String, botId: String, audio: VoiceAudio): VoiceSession? {
    val call = state.value.voice ?: return null
    if (call.id != id || call.botId != botId || voiceGeneration != state.value.generation || state.value.loading || voiceBinding != null) return null
    val generation = state.value.generation
    val entries = state.value.entries
    val head = maxOf(state.value.hello?.get("rev")?.jsonPrimitive?.longOrNull ?: 0, entries.maxOfOrNull(Entry::rev) ?: 0)
    val session = VoiceSession(contextScope, audio, call.state.settings) { text ->
        if (state.value.generation == generation && state.value.voice?.id == id && !state.value.loading) {
            val send = PendingSend(UUID.randomUUID().toString(), botId, null, text, System.currentTimeMillis())
            action("save:" + send.nonce) { savePending(send); deliver(send) }
        }
    }
    voiceBinding = VoiceBinding(id, botId, generation, android.os.SystemClock.elapsedRealtime(), session, VoiceReplyGate(botId, head, entries))
    contextScope.launch { session.state.takeWhile { it.phase != VoicePhase.Ended }.collect { status ->
        mutableState.update { current -> if (current.voice?.id == id) current.copy(voice = current.voice.copy(state = status)) else current }
    } }
    connect()
    return session
}
fun AppStore.endVoice(error: String? = null) {
    val id = state.value.voice?.id ?: return
    finishVoice(id, error); VoiceService.stop(getApplication(), id)
}
internal fun AppStore.finishVoice(id: String, error: String? = null) {
    if (state.value.voice?.id != id) return
    val call = voiceBinding?.takeIf { it.id == id }; voiceBinding = null; voiceGeneration = null
    call?.session?.end()
    mutableState.update { it.copy(voice = null, error = error ?: it.error) }
    PushDelivery.foreground = foreground
    contextScope.launch {
        try {
            call?.session?.awaitEnd()
            if (call != null && state.value.generation == call.generation && channel?.state?.value is LinkState.Ready) {
                val seconds = ((android.os.SystemClock.elapsedRealtime() - call.startedAt) / 1_000).coerceAtLeast(0)
                withTimeoutOrNull(3_000) { hostQuery("logCall", buildJsonObject { put("botId", call.botId); put("seconds", seconds) }, 3_000, call.generation) }
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* Duration is best effort; an uncertain log is never retried. */ }
        finally { if (!transportRequired) { connectionJob?.cancel(); graceJob?.cancel(); channel?.close() } }
    }
}
fun AppStore.muteVoice() { voiceBinding?.session?.let { it.mute(!it.state.value.muted) } }
fun AppStore.interruptVoice() { voiceBinding?.session?.interrupt() }
fun AppStore.resumeVoice() { voiceBinding?.session?.focus(true) }
fun AppStore.voiceSettings(settings: VoiceSettings) { VoicePreferences.save(getApplication(), settings); voiceBinding?.session?.settings(settings) }
