package com.codync.android

import android.os.SystemClock
import java.io.IOException
import java.util.Base64
import java.util.UUID
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*
import okhttp3.OkHttpClient

// A voice call with one bot: what you say is sent, its replies are read aloud. The engine is the one
// picked in the call settings; a cloud engine that can't start or drops mid-call hands over to on-device voice.

/** The call in progress: its engine (replaced on a fallback), start times and which replies it has read. */
internal class VoiceBinding(val id: String, val botId: String, val generation: Int, val startedAt: Long, val replies: VoiceReplyGate) {
    var engine: VoiceEngine? = null
    /** When the current cloud engine started, for this month's minutes. */
    var cloudSince: Long? = null
    /** The microphone's audio session, so Android silencing it pauses the call. */
    var recording: () -> Int? = { null }
    /** What the voice service sees: the current engine, never the failure it fell back from. */
    val state = MutableStateFlow(VoiceState())
}

private val voiceHttp by lazy { OkHttpClient() }

internal fun AppStore.voiceChoices(): VoiceChoices = VoicePreferences.choices(getApplication())

fun AppStore.startVoice(botId: String, expectedGeneration: Int = state.value.generation) {
    if (state.value.generation != expectedGeneration || state.value.loading || state.value.voice != null) return
    if (!foreground || state.value.actualConnection !is LinkState.Ready || state.value.bots.none { it.id == botId && !it.deleted }) {
        mutableState.update { it.copy(error = "Open the connected bot's conversation to start voice chat.") }; return
    }
    val id = UUID.randomUUID().toString()
    voiceGeneration = expectedGeneration
    val starting = VoiceState(settings = VoicePreferences.load(getApplication()), provider = voiceChoices().provider)
    mutableState.update { it.copy(voice = VoiceCallViewState(id, botId, starting)) }
    try { VoiceService.start(getApplication(), this, id, botId); PushDelivery.foreground = true }
    catch (_: Exception) { finishVoice(id, "Couldn't start voice chat. Allow microphone access and wait for any previous call to finish.") }
}

/** Starts the call's engine for the voice service; null when the call is gone. */
internal fun AppStore.attachVoice(id: String, botId: String): VoiceBinding? {
    val call = state.value.voice ?: return null
    if (call.id != id || call.botId != botId || voiceGeneration != state.value.generation || state.value.loading || voiceBinding != null) return null
    val entries = state.value.entries
    val head = maxOf(state.value.hello?.get("rev")?.jsonPrimitive?.longOrNull ?: 0, entries.maxOfOrNull(Entry::rev) ?: 0)
    val binding = VoiceBinding(id, botId, state.value.generation, SystemClock.elapsedRealtime(), VoiceReplyGate(botId, head, entries))
    voiceBinding = binding
    begin(binding, voiceChoices().provider)
    connect()
    return binding
}

/** Starts an engine: a provider on the key kept by the computer, or on-device speech. */
private fun AppStore.begin(call: VoiceBinding, provider: VoiceProvider?) {
    val choices = voiceChoices()
    // Picks up the computer's newest models for the next call; this one starts right away.
    if (provider != null) contextScope.launch { try { choices.remember(voiceStatus()) } catch (_: Exception) { } }
    val settings = VoicePreferences.load(getApplication())
    val operator = CallOperator(call.botId, object : CallTarget {
        override val bots get() = state.value.bots
        override val entries get() = state.value.entries
        override fun send(text: String) = sendFromCall(call, text)
        override fun respond(entry: Entry, optionId: String) = this@begin.respond(entry, optionId)
    })
    call.recording = { null }
    val engine: VoiceEngine = if (provider == null) {
        val audio = LocalVoiceAudio(getApplication())
        call.recording = { audio.recordingSessionId }
        VoiceSession(contextScope, audio, settings) { sendFromCall(call, it) }
    } else {
        val voice = choices.voice(provider)
        val model = choices.model(provider, VoiceChoices.Model.Realtime)
        val credential: suspend () -> String = { voiceSession(provider, model, voice) }
        fun mic() = AndroidMic(getApplication()).also { mic -> call.recording = { mic.recordingSessionId } }
        when {
            choices.speechMode(provider) -> {
                val transcribe = choices.model(provider, VoiceChoices.Model.Transcribe)
                val speech = choices.model(provider, VoiceChoices.Model.Speech)
                CloudSpeechEngine(contextScope, provider, settings, mic(), AndroidClipPlayer, CloudSpeechEngine.Speech(
                    transcribe = { inReadersScript(voiceTranscribe(provider, transcribe, it)) },
                    synthesize = { voiceSpeak(provider, speech, voice, it) },
                    send = { sendFromCall(call, it) }))
            }
            // WebRTC records the microphone itself.
            provider == VoiceProvider.OpenAI -> OpenAIRealtimeEngine(contextScope, settings, credential, operator, WebRtcPeer(getApplication())) { secret, offer ->
                OpenAIRealtimeEngine.answer(voiceHttp, secret, offer)
            }
            else -> GeminiLiveEngine(contextScope, settings, model, voice, credential, operator, mic(), AndroidPcmPlayer(), voiceHttp)
        }
    }
    call.engine = engine
    call.cloudSince = if (provider != null) SystemClock.elapsedRealtime() else null
    contextScope.launch {
        val last = engine.state.first { status ->
            val current = voiceBinding === call && call.engine === engine
            val fallingBack = provider != null && status.phase == VoicePhase.Failed
            if (current && !fallingBack) publish(call, status)
            !current || fallingBack || status.phase == VoicePhase.Ended
        }
        if (provider == null || last.phase != VoicePhase.Failed || voiceBinding !== call) return@launch
        countCloudMinutes(call)
        val notice = "${provider.title} isn't available (${last.error ?: "no answer"}). Using on-device voice."
        mutableState.update { it.copy(voice = it.voice?.takeIf { voice -> voice.id == call.id }?.copy(notice = notice) ?: it.voice) }
        engine.end(); engine.awaitEnd()
        if (voiceBinding === call) begin(call, null)
    }
}

private fun AppStore.publish(call: VoiceBinding, status: VoiceState) {
    call.state.value = status
    mutableState.update { current -> if (current.voice?.id == call.id) current.copy(voice = current.voice.copy(state = status)) else current }
}

private fun AppStore.countCloudMinutes(call: VoiceBinding) {
    val provider = call.engine?.state?.value?.provider ?: return
    val since = call.cloudSince ?: return
    call.cloudSince = null
    voiceChoices().addUsage(provider, (SystemClock.elapsedRealtime() - since) / 1_000)
}

/** What you said goes out as an ordinary message, unless the call has moved on. */
private fun AppStore.sendFromCall(call: VoiceBinding, text: String) {
    if (state.value.generation != call.generation || state.value.voice?.id != call.id || state.value.loading) return
    val send = PendingSend(UUID.randomUUID().toString(), call.botId, null, text, System.currentTimeMillis())
    action("save:" + send.nonce) { savePending(send); deliver(send) }
}

/** New final replies and approval notices for the call to say. */
internal fun AppStore.speakToCall(entries: List<Entry>, bots: List<Bot>) {
    val call = voiceBinding ?: return
    if (bots.none { it.id == call.botId && !it.deleted }) { endVoice("Voice chat ended because its bot was removed."); return }
    val engine = call.engine
    for (cue in call.replies.observe(entries, bots)) if (cue.notice) engine?.announce(cue.text) else engine?.speak(cue.text)
}

fun AppStore.endVoice(error: String? = null) {
    val id = state.value.voice?.id ?: return
    finishVoice(id, error); VoiceService.stop(getApplication(), id)
}

internal fun AppStore.finishVoice(id: String, error: String? = null) {
    if (state.value.voice?.id != id) return
    val call = voiceBinding?.takeIf { it.id == id }; voiceBinding = null; voiceGeneration = null
    val engine = call?.engine
    engine?.end()
    if (call != null) countCloudMinutes(call)
    mutableState.update { it.copy(voice = null, error = error ?: it.error) }
    PushDelivery.foreground = foreground
    contextScope.launch {
        try {
            engine?.awaitEnd()
            if (call != null && state.value.generation == call.generation && channel?.state?.value is LinkState.Ready) {
                val seconds = ((SystemClock.elapsedRealtime() - call.startedAt) / 1_000).coerceAtLeast(0)
                withTimeoutOrNull(3_000) { hostQuery("logCall", buildJsonObject { put("botId", call.botId); put("seconds", seconds) }, 3_000, call.generation) }
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* Duration is best effort; an uncertain log is never retried. */ }
        finally { if (!transportRequired) { connectionJob?.cancel(); graceJob?.cancel(); channel?.close() } }
    }
}

/** Ends the engine of a retiring account and waits for its audio to close. */
internal suspend fun AppStore.retireVoice() {
    val call = voiceBinding; voiceBinding = null; voiceGeneration = null
    val callId = state.value.voice?.id
    mutableState.update { it.copy(voice = null) }
    if (callId != null) VoiceService.stop(getApplication(), callId)
    call?.engine?.end(); call?.engine?.awaitEnd()
}

fun AppStore.muteVoice() { voiceBinding?.engine?.let { it.mute(!it.state.value.muted) } }
fun AppStore.interruptVoice() { voiceBinding?.engine?.interrupt() }
fun AppStore.resumeVoice() { voiceBinding?.engine?.focus(true) }
internal fun AppStore.focusVoice(active: Boolean) { voiceBinding?.engine?.focus(active) }
internal fun AppStore.voiceRecordingSession(): Int? = voiceBinding?.recording?.invoke()
fun AppStore.voiceSettings(settings: VoiceSettings) { VoicePreferences.save(getApplication(), settings); voiceBinding?.engine?.settings(settings) }

// The computer's voice methods: it holds the provider keys and does the provider calls that need them.

internal suspend fun AppStore.voiceStatus(): VoiceStatus = WireJson.decodeFromJsonElement(hostCall("voiceStatus", JsonObject(emptyMap()), 30_000))
internal suspend fun AppStore.voiceModels(provider: VoiceProvider): VoiceModels =
    WireJson.decodeFromJsonElement(hostCall("voiceModels", body("provider" to provider.id), 30_000))

/** A short-lived credential for one call's audio session. */
internal suspend fun AppStore.voiceSession(provider: VoiceProvider, model: String, voice: String): String =
    hostCall("voiceSession", body("provider" to provider.id, "model" to model, "voice" to voice), 30_000).jsonObject["credential"]
        ?.jsonPrimitive?.contentOrNull ?: throw IOException("${provider.title} didn't start the call.")

/** Speech to text for one utterance (a WAV file), with the computer's key. */
internal suspend fun AppStore.voiceTranscribe(provider: VoiceProvider, model: String, wav: ByteArray): String =
    hostCall("voiceTranscribe", body("provider" to provider.id, "model" to model, "audio" to Base64.getEncoder().encodeToString(wav)), 60_000)
        .jsonObject["text"]?.jsonPrimitive?.contentOrNull.orEmpty()

/** Text to speech for a stretch of a reply: the audio and its format (`aac` or `wav`). */
internal suspend fun AppStore.voiceSpeak(provider: VoiceProvider, model: String, voice: String, text: String): Pair<ByteArray, String> {
    val clip = hostCall("voiceSpeak", body("provider" to provider.id, "model" to model, "voice" to voice, "text" to text), 90_000).jsonObject
    val audio = clip["audio"]?.jsonPrimitive?.contentOrNull?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
        ?: throw IOException("Unreadable audio.")
    return audio to (clip["format"]?.jsonPrimitive?.contentOrNull ?: "aac")
}
