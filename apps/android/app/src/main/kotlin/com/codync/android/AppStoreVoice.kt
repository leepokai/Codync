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

// A voice call with one bot, run by the computer (`HostCallLink`): what you say reaches the bot as the call's
// messages, its replies are read aloud, and the call ends as one titled line. The engine is the one picked in the
// call settings; a cloud engine that can't start or drops mid-call hands over to on-device voice.

/** The call in progress: its engine (replaced on a fallback) and start times. */
internal class VoiceBinding(val id: String, val botId: String, val generation: Int, val startedAt: Long) {
    var engine: VoiceEngine? = null
    /** The newest cue the call heard (`CallCue.seq`), kept across engines: a replayed one isn't said again. */
    var heard = 0L
    /** When the current cloud engine started, for this month's minutes. */
    var cloudSince: Long? = null
    /** The call settings the current engine was built from. */
    var choice = ""
    /** The microphone's audio session, so Android silencing it pauses the call. */
    var recording: () -> Int? = { null }
    /** What the voice service sees: the current engine, never the failure it fell back from. */
    val state = MutableStateFlow(VoiceState())
    /** What the bot is doing, from the computer's progress cues, for the call's notification; cleared by its reply. */
    val progress = MutableStateFlow<String?>(null)
    /** `voice_call_started` (and `callStarted`) went out: once per call, with the engine that got it going. */
    var reported = false
    /** The call on the computer. */
    var link: HostCallLink? = null
    /** Each engine's `voice_call_quality`. */
    val quality = CallQuality<VoiceEngineKind> { kind, report -> Analytics.capture(AnalyticsEvent.VoiceCallQuality(kind, report)) }
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
    val binding = VoiceBinding(id, botId, state.value.generation, SystemClock.elapsedRealtime())
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
    call.choice = choices.engineChoice(provider, callLanguage())
    // A muted call stays muted on the engine that takes over (what the call last showed).
    val muted = call.state.value.muted
    val link = call.link(this)
    call.recording = { null }
    val kind = when {
        provider == null -> VoiceEngineKind.OnDevice
        choices.speechMode(provider) -> VoiceEngineKind.Cloud
        provider == VoiceProvider.OpenAI -> VoiceEngineKind.OpenAI
        else -> VoiceEngineKind.Gemini
    }
    val engine: VoiceEngine = if (provider == null) {
        val audio = LocalVoiceAudio(getApplication(), VoicePreferences.language(getApplication()))
        call.recording = { audio.recordingSessionId }
        startCallOnHost(call, "device", null)
        VoiceSession(contextScope, audio, settings) { sendFromCall(call, it) }
    } else {
        val voice = choices.voice(provider)
        val model = choices.model(provider, VoiceChoices.Model.Realtime)
        fun mic() = AndroidMic(getApplication()).also { mic -> call.recording = { mic.recordingSessionId } }
        when {
            choices.speechMode(provider) -> {
                startCallOnHost(call, "speech", provider)
                val transcribe = choices.model(provider, VoiceChoices.Model.Transcribe)
                val speech = choices.model(provider, VoiceChoices.Model.Speech)
                CloudSpeechEngine(contextScope, provider, settings, mic(), AndroidClipPlayer, CloudSpeechEngine.Speech(
                    transcribe = { inReadersScript(voiceTranscribe(provider, transcribe, it)) },
                    synthesize = { voiceSpeak(provider, speech, voice, it) },
                    send = { sendFromCall(call, it) }))
            }
            // WebRTC records the microphone itself.
            provider == VoiceProvider.OpenAI && isOpenAILiveModel(model) ->
                OpenAILiveEngine(contextScope, settings, model, voice, link, WebRtcPeer(getApplication()))
            provider == VoiceProvider.OpenAI -> OpenAIRealtimeEngine(contextScope, settings, model, voice,
                choices.model(provider, VoiceChoices.Model.Transcribe), link, WebRtcPeer(getApplication())) { secret, offer ->
                OpenAIRealtimeEngine.answer(voiceHttp, secret, offer)
            }
            else -> GeminiLiveEngine(contextScope, settings, model, voice, link, mic(), AndroidPcmPlayer(), voiceHttp)
        }
    }
    call.engine = engine
    call.quality.begin(kind)
    (engine as? GeminiLiveEngine)?.onStall = { contextScope.launch { if (call.engine === engine) call.quality.stalled() } }
    if (muted) engine.mute(true)
    call.cloudSince = if (provider != null) SystemClock.elapsedRealtime() else null
    contextScope.launch {
        val last = engine.state.first { status ->
            val current = voiceBinding === call && call.engine === engine
            if (current) call.quality.phase(status.phase, status.failureKind)
            val fallingBack = provider != null && status.phase == VoicePhase.Failed
            if (current && !fallingBack) {
                publish(call, status)
                // Live once it listens or talks; a cloud engine that failed first is reported as the one that took over.
                if (!call.reported && (status.phase == VoicePhase.Listening || status.phase == VoicePhase.Speaking)) {
                    call.reported = true
                    Analytics.capture(AnalyticsEvent.VoiceCallStarted(kind))
                    callStarted(call)
                }
            }
            !current || fallingBack || status.phase == VoicePhase.Ended
        }
        if (provider == null || last.phase != VoicePhase.Failed || voiceBinding !== call || call.engine !== engine) return@launch
        handOver(call, "${provider.title} isn't available (${last.error ?: "no answer"}). Using on-device voice.") { null }
    }
}

/** The call's audio is live: the lead speaks first, on the call (only a host with leads marks one, so an older host never gets this). */
private fun AppStore.callStarted(call: VoiceBinding) {
    if (state.value.bots.any { it.id == call.botId && it.lead }) command("callStarted", body("botId" to call.botId, "callId" to call.id), "callStarted:" + call.botId)
}

/** After the call settings close: a different engine picked there takes over the call, which goes on. */
fun AppStore.switchVoiceEngine() {
    val call = voiceBinding ?: return
    val choices = voiceChoices()
    // No engine: a hand-over is under way; a switch begins what is picked by the time it's done.
    if (call.engine == null || choices.engineChoice(choices.provider, callLanguage()) == call.choice) return
    handOver(call, null) { voiceChoices().provider }
}

/**
 * Ends the call's engine and, once it has closed, begins `next()` in the same call, unless the call
 * ended or got an engine meanwhile. The old engine is detached first: its end mustn't reach the
 * call's state, which would end the call.
 */
private fun AppStore.handOver(call: VoiceBinding, notice: String?, next: () -> VoiceProvider?) {
    val old = call.engine
    countCloudMinutes(call)
    call.engine = null
    mutableState.update { it.copy(voice = it.voice?.takeIf { voice -> voice.id == call.id }?.copy(notice = notice) ?: it.voice) }
    contextScope.launch {
        old?.end(); old?.awaitEnd()
        if (voiceBinding === call && call.engine == null) begin(call, next())
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

/** What you said goes out as the call's message, unless the call has moved on. */
private fun AppStore.sendFromCall(call: VoiceBinding, text: String) {
    if (state.value.generation != call.generation || state.value.voice?.id != call.id || state.value.loading) return
    val send = PendingSend(UUID.randomUUID().toString(), call.botId, text, System.currentTimeMillis(), call = call.id)
    action("save:" + send.nonce) { savePending(send); deliver(send) }
}

/** What the computer tells the call's voice (host chat/calls/announce), each cue once: the engine only says it. */
internal fun AppStore.hearCalls(events: List<MirrorEvent>) {
    val call = voiceBinding ?: return
    for (cue in events.filterIsInstance<MirrorEvent.Call>().map { it.cue }) {
        val engine = call.engine
        if (cue.call != call.id || cue.seq <= call.heard || engine == null) continue
        call.heard = cue.seq
        engine.hear(cue)
        when (cue.kind) {
            CallCue.Kind.Progress -> call.progress.value = cue.text
            CallCue.Kind.Reply -> call.progress.value = null
            CallCue.Kind.Notice -> {}
        }
    }
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
    call?.quality?.report()
    if (call != null) countCloudMinutes(call)
    mutableState.update { it.copy(voice = null, error = error ?: it.error) }
    PushDelivery.foreground = foreground
    contextScope.launch {
        try {
            engine?.awaitEnd()
            if (call != null && state.value.generation == call.generation) {
                call.link?.end(((SystemClock.elapsedRealtime() - call.startedAt) / 1_000).coerceAtLeast(0))
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* The call's line is best effort once the link is gone. */ }
        finally { if (!transportRequired) { connectionJob?.cancel(); graceJob?.cancel(); channel?.close() } }
    }
}

/** Ends the engine of a retiring account and waits for its audio to close. */
internal suspend fun AppStore.retireVoice() {
    val call = voiceBinding; voiceBinding = null; voiceGeneration = null
    val callId = state.value.voice?.id
    mutableState.update { it.copy(voice = null) }
    if (callId != null) VoiceService.stop(getApplication(), callId)
    call?.quality?.report()
    call?.engine?.end(); call?.engine?.awaitEnd()
}

fun AppStore.muteVoice() { voiceBinding?.engine?.let { it.mute(!it.state.value.muted) } }
fun AppStore.interruptVoice() { voiceBinding?.engine?.interrupt() }
fun AppStore.resumeVoice() { voiceBinding?.engine?.focus(true) }
internal fun AppStore.focusVoice(active: Boolean) { voiceBinding?.engine?.focus(active) }
internal fun AppStore.voiceRecordingSession(): Int? = voiceBinding?.recording?.invoke()
/** The language on-device calls hear; empty follows the phone. */
internal fun AppStore.callLanguage(): String = VoicePreferences.language(getApplication())

/** Picks it, and has Android fetch it when it hears it on device but hasn't downloaded it yet. */
internal fun AppStore.callLanguage(tag: String) {
    VoicePreferences.language(getApplication(), tag)
    if (tag.isNotEmpty()) contextScope.launch {
        val support = PlatformSpeech.support(getApplication())
        if (tag !in support.installed && tag in support.downloadable) PlatformSpeech.download(getApplication(), tag)
    }
}

/** What on-device calls can hear on this phone: Android's on-device languages, and English always. */
internal suspend fun AppStore.callLanguages(): List<String> {
    val onDevice = PlatformSpeech.support(getApplication()).onDevice
    return if (onDevice.any { VoiceLanguages.base(java.util.Locale.forLanguageTag(it)) == "en" }) onDevice else onDevice + VoiceLanguages.ENGLISH
}

fun AppStore.voiceSettings(settings: VoiceSettings) { VoicePreferences.save(getApplication(), settings); voiceBinding?.engine?.settings(settings) }

// The computer's voice methods: it holds the provider keys and does the provider calls that need them.

internal suspend fun AppStore.voiceStatus(): VoiceStatus = WireJson.decodeFromJsonElement(hostCall("voiceStatus", JsonObject(emptyMap()), 30_000))
internal suspend fun AppStore.voiceModels(provider: VoiceProvider): VoiceModels =
    WireJson.decodeFromJsonElement(hostCall("voiceModels", buildJsonObject { put("provider", provider.id); put("supportsLive", true) }, 30_000))

/** Checks model access without creating a billable Live session; Realtime still mints its credential. */
internal suspend fun AppStore.voiceSession(provider: VoiceProvider, model: String, voice: String): String {
    if (provider == VoiceProvider.OpenAI && isOpenAILiveModel(model)) {
        requireOpenAILiveSupport(hostCall("voiceStatus", JsonObject(emptyMap()), 30_000).jsonObject)
    }
    val result = hostCall("voiceSession", body("provider" to provider.id, "model" to model, "voice" to voice), 30_000).jsonObject
    if (provider == VoiceProvider.OpenAI && isOpenAILiveModel(model) && result["validated"]?.jsonPrimitive?.booleanOrNull == true) return "validated"
    return result["credential"]?.jsonPrimitive?.contentOrNull ?: throw IOException("${provider.title} didn't validate the call model.")
}

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
