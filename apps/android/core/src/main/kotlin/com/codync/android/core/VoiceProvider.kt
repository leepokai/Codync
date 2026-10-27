package com.codync.android.core

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.time.YearMonth
import kotlin.math.log10
import kotlinx.serialization.Serializable

// Realtime voice on the user's own provider keys (host/src/voice.rs). The keys stay on the
// computer; a call asks the host for a short-lived credential and streams audio to the provider.

enum class VoiceProvider(val id: String, val title: String, val keyPage: String, val keyPrompt: String,
    /** Used until the user picks one, and until the computer has said what's newest (`VoiceDefaults`). */
    val model: String, val transcribeModel: String, val speechModel: String,
    /** Longest stretch of a reply turned into audio at once: each clip has to fit one channel message (1 MiB). */
    val speechChunk: Int, val voices: List<String>) {
    OpenAI("openai", "OpenAI", "https://platform.openai.com/api-keys", "sk-…", "gpt-realtime-2.1", "gpt-transcribe",
        "gpt-4o-mini-tts", 600, listOf("marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse")),
    Gemini("gemini", "Gemini", "https://aistudio.google.com/apikey", "AIza…", "gemini-3.8-live", "gemini-3.8-flash",
        "gemini-3.8-flash-tts", 150, listOf("Kore", "Puck", "Charon", "Fenrir", "Aoede", "Leda", "Orus", "Zephyr"));

    val defaults: VoiceDefaults get() = VoiceDefaults(model, transcribeModel, speechModel)

    companion object { fun of(id: String?): VoiceProvider? = entries.firstOrNull { it.id == id } }
}

/** The provider's newest model for each voice mode, as the computer last found them. */
@Serializable data class VoiceDefaults(val realtime: String, val transcribe: String, val speech: String)

@Serializable data class VoiceStatus(val providers: List<ProviderStatus> = emptyList(), val googleSearch: Boolean = false) {
    /** `provider` stays a string so a provider added by a newer computer doesn't break the list. */
    @Serializable data class ProviderStatus(val provider: String, val configured: Boolean = false, val defaults: VoiceDefaults? = null)
    fun configured(provider: VoiceProvider): Boolean = providers.any { it.provider == provider.id && it.configured }
}

/** The models the computer's key can use, per voice mode. */
@Serializable data class VoiceModels(val models: List<String> = emptyList(), val transcribe: List<String> = emptyList(),
    val speech: List<String> = emptyList(), val defaults: VoiceDefaults? = null)

/**
 * The call settings kept on this phone (iOS `VoiceSettings`): the engine, each provider's mode,
 * models and voice, and this month's minutes. A default model isn't stored, so a newer one reaches the user.
 */
class VoiceChoices(private val read: (String) -> String?, private val write: (String, String?) -> Unit) {
    enum class Model(val prefix: String) { Realtime("callModel"), Transcribe("callSpeechToText"), Speech("callSpeechModel") }

    var provider: VoiceProvider?
        get() = VoiceProvider.of(read("callEngine"))
        set(value) = write("callEngine", value?.id)

    var googleSearch: Boolean
        get() = read("callGoogleSearch.gemini") == "true"
        set(value) = write("callGoogleSearch.gemini", value.toString())

    fun defaults(provider: VoiceProvider): VoiceDefaults = read("callDefaults.${provider.id}")
        ?.let { runCatching { WireJson.decodeFromString(VoiceDefaults.serializer(), it) }.getOrNull() } ?: provider.defaults
    fun remember(defaults: VoiceDefaults?, provider: VoiceProvider) {
        if (defaults != null) write("callDefaults.${provider.id}", WireJson.encodeToString(VoiceDefaults.serializer(), defaults))
    }
    fun remember(status: VoiceStatus) { for (entry in status.providers) VoiceProvider.of(entry.provider)?.let { remember(entry.defaults, it) } }

    /** Speech: speech to text, then the reply read aloud. Otherwise a realtime conversation. */
    fun speechMode(provider: VoiceProvider): Boolean = read("callMode.${provider.id}") == "speech"
    fun speechMode(provider: VoiceProvider, on: Boolean) = write("callMode.${provider.id}", if (on) "speech" else "realtime")

    fun model(provider: VoiceProvider, kind: Model): String = read("${kind.prefix}.${provider.id}") ?: fallback(provider, kind)
    fun choose(provider: VoiceProvider, kind: Model, model: String) =
        write("${kind.prefix}.${provider.id}", model.takeUnless { it.isEmpty() || it == fallback(provider, kind) })
    private fun fallback(provider: VoiceProvider, kind: Model) = defaults(provider).let {
        when (kind) { Model.Realtime -> it.realtime; Model.Transcribe -> it.transcribe; Model.Speech -> it.speech }
    }

    fun voice(provider: VoiceProvider): String = read("callVoice.${provider.id}")?.takeIf { it in provider.voices } ?: provider.voices[0]
    fun voice(provider: VoiceProvider, voice: String) = write("callVoice.${provider.id}", voice)

    /** What a call's engine is built from (`language`: on device): a different one picked mid-call takes over the call. */
    fun engineChoice(provider: VoiceProvider?, language: String): String = when {
        provider == null -> "device $language"
        speechMode(provider) -> "${provider.id} speech ${model(provider, Model.Transcribe)} ${model(provider, Model.Speech)} ${voice(provider)}"
        else -> "${provider.id} ${model(provider, Model.Realtime)} ${voice(provider)} ${provider == VoiceProvider.Gemini && googleSearch}"
    }

    /** The provider's model list as last fetched with the user's key. */
    fun cachedModels(provider: VoiceProvider): VoiceModels? = read("callModels.${provider.id}")
        ?.let { runCatching { WireJson.decodeFromString(VoiceModels.serializer(), it) }.getOrNull() }
    fun cache(provider: VoiceProvider, models: VoiceModels) = write("callModels.${provider.id}", WireJson.encodeToString(VoiceModels.serializer(), models))

    /** Cloud voice used this month, counted on this device: an estimate, not a bill. */
    fun addUsage(provider: VoiceProvider, seconds: Long, month: YearMonth = YearMonth.now()) {
        if (seconds <= 0) return
        write(usageKey(provider, month), (usedSeconds(provider, month) + seconds).toString())
    }
    fun minutes(provider: VoiceProvider, month: YearMonth = YearMonth.now()): Long = (usedSeconds(provider, month) + 59) / 60
    private fun usedSeconds(provider: VoiceProvider, month: YearMonth) = read(usageKey(provider, month))?.toLongOrNull() ?: 0
    private fun usageKey(provider: VoiceProvider, month: YearMonth) = "callUsage.${provider.id}.$month"
}

/** Microphone audio: 16 kHz mono PCM16, little-endian. */
object Pcm {
    const val RATE = 16_000

    /** Loudness, 0…1: -50 dB (a quiet room) … 0 dB. */
    fun level(samples: ShortArray, count: Int): Float {
        if (count <= 0) return 0f
        var power = 0.0
        for (i in 0 until count) { val sample = samples[i] / 32_768.0; power += sample * sample }
        val db = 10 * log10(maxOf(power / count, 1e-10))
        return ((db + 50) / 50).toFloat().coerceIn(0f, 1f)
    }

    fun bytes(samples: ShortArray, count: Int): ByteArray = ByteBuffer.allocate(count * 2).order(ByteOrder.LITTLE_ENDIAN)
        .apply { asShortBuffer().put(samples, 0, count) }.array()

    /** PCM wrapped in a WAV header. */
    fun wav(pcm: ByteArray, rate: Int = RATE): ByteArray = ByteBuffer.allocate(44 + pcm.size).order(ByteOrder.LITTLE_ENDIAN).apply {
        put("RIFF".toByteArray()); putInt(36 + pcm.size); put("WAVEfmt ".toByteArray()); putInt(16)
        putShort(1); putShort(1); putInt(rate); putInt(rate * 2); putShort(2); putShort(16)
        put("data".toByteArray()); putInt(pcm.size); put(pcm)
    }.array()
}

/** Sentences grouped into clips of at most `limit` characters (longer sentences are cut). */
fun speechClips(text: String, limit: Int): List<String> {
    val out = mutableListOf<String>(); val current = StringBuilder(); val sentence = StringBuilder()
    fun flush() { val clip = current.trim(); if (clip.isNotEmpty()) out += clip.toString(); current.clear() }
    fun add(piece: String) {
        if (current.length + piece.length > limit) flush()
        var rest = piece
        while (rest.length > limit) { val cut = SpokenText.chunks(rest, limit).first(); out += cut; rest = rest.substring(cut.length) }
        current.append(rest)
    }
    for (character in text) {
        sentence.append(character)
        if (character in ".!?。！？\n") { add(sentence.toString()); sentence.clear() }
    }
    add(sentence.toString()); flush()
    return out
}

/**
 * Cuts the microphone into utterances at a pause, for speech to text: the PCM since speech began
 * (plus a little before), sent once it has gone quiet for `pauseMillis` or reached the size one
 * channel message holds. Under 0.4 s of voice is a cough or noise (and speech-to-text models
 * invent words for noise), so it is dropped.
 */
class UtteranceDetector {
    private var preroll = ByteArray(0)
    private val utterance = ByteArrayOutputStream()
    private var speaking = false
    private var quietSince: Long? = null
    /** How much of the utterance was actually loud, in bytes of PCM. */
    private var voiced = 0

    /** Takes one frame; returns the finished utterance as WAV, or null. */
    fun feed(pcm: ByteArray, level: Float, now: Long, pauseMillis: Long): ByteArray? {
        if (level > THRESHOLD) { speaking = true; quietSince = null; voiced += pcm.size }
        else if (speaking && quietSince == null) quietSince = now
        if (!speaking) {
            preroll = (preroll + pcm).let { if (it.size > PREROLL) it.copyOfRange(it.size - PREROLL, it.size) else it }
            return null
        }
        utterance.write(pcm)
        val paused = quietSince?.let { now - it >= pauseMillis } ?: false
        if (!paused && utterance.size() < MAX_BYTES) return null
        val wav = Pcm.wav(preroll + utterance.toByteArray())
        val enough = voiced >= MIN_VOICED
        reset()
        return wav.takeIf { enough }
    }

    fun reset() { preroll = ByteArray(0); utterance.reset(); speaking = false; quietSince = null; voiced = 0 }

    companion object {
        const val THRESHOLD = .3f
        private const val PREROLL = Pcm.RATE * 2 * 3 / 10
        /** An utterance has to fit one channel message once base64-encoded. */
        const val MAX_BYTES = Pcm.RATE * 2 * 20
        private const val MIN_VOICED = Pcm.RATE * 2 * 4 / 10
    }
}
