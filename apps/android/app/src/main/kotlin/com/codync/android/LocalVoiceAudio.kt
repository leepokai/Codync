package com.codync.android

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.*
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import androidx.core.content.ContextCompat
import androidx.core.content.edit
import com.codync.android.core.*
import java.io.File
import java.io.IOException
import java.util.Locale
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject
import org.vosk.LibVosk
import org.vosk.Model
import org.vosk.Recognizer

/**
 * On-device calls: the call's language heard by Android's on-device recognizer, or the bundled English
 * model, and each reply read by an installed offline voice for the language it's written in. No PCM
 * is written to disk.
 */
internal class LocalVoiceAudio(private val context: Context, private val picked: String) : VoiceAudio {
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var model: Model? = null
    private val mic = AndroidMic(context)
    /** Vosk, fed from the microphone's thread; closed only after that thread has stopped. */
    private var decoder: Recognizer? = null
    /** Android's recognizer, when it hears the call's language. */
    private var platform: PlatformSpeechInput? = null
    private var language = VoiceLanguages.ENGLISH
    private var speech: TextToSpeech? = null
    private var voices: List<android.speech.tts.Voice> = emptyList()
    private var completion: ((Long, String?) -> Unit)? = null
    private var speakingId: Long? = null; private var lastChunk: String? = null
    val recordingSessionId: Int? get() = mic.recordingSessionId

    override suspend fun initialize(): String {
        require(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        val support = PlatformSpeech.support(context)
        language = VoiceLanguages.resolve(picked, Locale.getDefault(), support.onDevice)
        val recognition = VoiceLanguages.recognition(language, support.onDevice)
            ?: throw VoiceUnavailable("Speech recognition isn't available for this language right now.")
        if (recognition == Recognition.Platform && Build.VERSION.SDK_INT >= 33) {
            if (language !in support.installed) {
                PlatformSpeech.download(context, language)
                throw VoiceUnavailable("Android is getting ${PlatformSpeech.name(language)} speech ready on this phone. Try the call again in a few minutes.")
            }
            platform = PlatformSpeechInput(context, language, mic)
        } else withContext(Dispatchers.IO) {
            try {
                LibVosk.vosk_set_log_level(-1)
                val directory = VoskModels.english(context)
                currentCoroutineContext().ensureActive()
                model = Model(directory.absolutePath)
            } catch (error: LinkageError) { throw IOException("The local speech engine couldn't load.", error) }
        }
        val ready = CompletableDeferred<Int>()
        val engine = TextToSpeech(context) { ready.complete(it) }
        speech = engine
        val status = withTimeoutOrNull(20_000) { ready.await() }
            ?: throw VoiceUnavailable("Android's text-to-speech engine didn't start. Try the call again.")
        if (status != TextToSpeech.SUCCESS) throw VoiceUnavailable("Install a text-to-speech engine in Android's speech settings.")
        voices = engine.voices.orEmpty().filter { !it.isNetworkConnectionRequired && TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED !in it.features.orEmpty() }
        if (voice(language) == null && voice(VoiceLanguages.ENGLISH) == null)
            throw VoiceUnavailable("Install an offline voice in Android's text-to-speech settings.")
        engine.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) { }
            override fun onDone(utteranceId: String?) { main.post {
                if (utteranceId == lastChunk) { val id = speakingId; val callback = completion
                    speakingId = null; lastChunk = null; completion = null; if (id != null) callback?.invoke(id, null) }
            } }
            @Deprecated("Required by older TTS engines") override fun onError(utteranceId: String?) { failed(utteranceId) }
            override fun onError(utteranceId: String?, errorCode: Int) { failed(utteranceId) }
            private fun failed(utteranceId: String?) { main.post {
                val id = speakingId ?: return@post
                if (utteranceId?.startsWith("codync-$id-") != true) return@post
                val callback = completion; speakingId = null; lastChunk = null; completion = null
                callback?.invoke(id, "Couldn't read the reply. Check the installed offline voice.")
            } }
        })
        return if (platform != null) "${PlatformSpeech.name(language)} · Android speech" else "Offline English · Vosk"
    }
    override suspend fun listen(generation: Long, events: (VoiceInput) -> Unit) {
        val platform = platform
        if (platform != null && Build.VERSION.SDK_INT >= 33) { platform.listen(generation, events); return }
        check(decoder == null)
        val recognizer = Recognizer(requireNotNull(model), Pcm.RATE.toFloat()).also { decoder = it }
        var previous = ""
        try {
            mic.start({ pcm, level ->
                events(VoiceInput.Level(generation, level))
                if (recognizer.acceptWaveForm(pcm, pcm.size)) {
                    val text = JSONObject(recognizer.result).optString("text")
                    previous = ""; if (text.isNotBlank()) events(VoiceInput.Text(generation, text, true))
                } else {
                    val text = JSONObject(recognizer.partialResult).optString("partial")
                    if (text != previous) { previous = text; events(VoiceInput.Text(generation, text, false)) }
                }
            }) { events(VoiceInput.Failed(generation, "Couldn't use the microphone. Check its permission and other audio apps.")) }
        } catch (error: Exception) {
            decoder = null; recognizer.close()
            events(VoiceInput.Failed(generation, error.message ?: "Couldn't use the microphone."))
        }
    }
    override suspend fun stopListening() {
        withContext(NonCancellable) {
            if (Build.VERSION.SDK_INT >= 33) platform?.stop()
            mic.stop(); decoder?.close(); decoder = null
        }
    }
    override suspend fun speak(id: Long, text: String, rate: Float, completed: (Long, String?) -> Unit) {
        val engine = requireNotNull(speech)
        val chunks = SpokenText.chunks(text)
        speakingId = id; completion = completed; lastChunk = "codync-$id-${chunks.lastIndex}"
        val written = VoiceLanguages.spoken(text, language)
        // A reply in a language with no installed voice would come out as noise: say what to install instead.
        val voice = voice(written) ?: throw VoiceUnavailable("Install an offline ${PlatformSpeech.name(written)} voice in Android's text-to-speech settings to hear this reply.")
        check(engine.setVoice(voice) == TextToSpeech.SUCCESS)
        check(engine.setSpeechRate(rate) == TextToSpeech.SUCCESS)
        for ((index, chunk) in chunks.withIndex()) {
            check(engine.speak(chunk, TextToSpeech.QUEUE_ADD, Bundle(), "codync-$id-$index") == TextToSpeech.SUCCESS)
        }
    }

    /** The best installed offline voice for a language: its region (for Chinese, the reader's script), then quality. */
    private fun voice(tag: String): android.speech.tts.Voice? {
        val wanted = Locale.forLanguageTag(tag)
        val base = VoiceLanguages.base(wanted)
        val region = wanted.country.ifEmpty { if (base == "zh") (if (readsTraditional()) "TW" else "CN") else Locale.getDefault().country }
        return voices.filter { VoiceLanguages.base(it.locale) == base }
            .maxWithOrNull(compareBy<android.speech.tts.Voice> { it.locale.country == region }.thenBy { it.quality })
    }
    override suspend fun stopSpeaking() { completion = null; speakingId = null; lastChunk = null; speech?.stop() }
    override suspend fun close() {
        try { stopListening() }
        finally {
            try { stopSpeaking() }
            finally {
                val engine = speech; speech = null
                try { engine?.shutdown() }
                finally { withContext(Dispatchers.IO) { val old = model; model = null; old?.close() } }
            }
        }
    }
}

/** Public linguistic weights are shared; account credentials and captured audio never enter this directory. */
internal object VoskModels {
    private val lock = Mutex()
    private const val source = "voice/vosk-model-small-en-us-0.15"
    suspend fun english(context: Context): File = lock.withLock {
        val destination = File(context.noBackupFilesDir, "voice-models/en-us-0.15")
        if (File(destination, ".complete").isFile) return@withLock destination
        val temporary = File(destination.parentFile, "en-us-0.15-pending")
        temporary.deleteRecursively(); temporary.mkdirs()
        suspend fun copy(folder: String, target: File) {
            val children = context.assets.list(folder).orEmpty()
            for (child in children) {
                currentCoroutineContext().ensureActive()
                val path = "$folder/$child"; val output = File(target, child)
                if (context.assets.list(path).orEmpty().isNotEmpty()) { output.mkdirs(); copy(path, output) }
                else context.assets.open(path).use { input -> output.outputStream().use { input.copyTo(it) } }
            }
        }
        try {
            copy(source, temporary)
            check(File(temporary, "am/final.mdl").isFile && File(temporary, "conf/model.conf").isFile)
            File(temporary, ".complete").writeText("0.15")
            destination.deleteRecursively(); check(temporary.renameTo(destination))
            destination
        } finally { temporary.deleteRecursively() }
    }
}

/** Call settings on this phone: the pause, reading speed and language, and the cloud engine choices (`VoiceChoices`). */
internal object VoicePreferences {
    private fun stored(context: Context) = context.getSharedPreferences("codync-voice", Context.MODE_PRIVATE)
    fun load(context: Context): VoiceSettings {
        val stored = stored(context)
        return runCatching { VoiceSettings(stored.getLong("pause", 1_500), stored.getFloat("rate", 1f)) }.getOrDefault(VoiceSettings())
    }
    fun save(context: Context, settings: VoiceSettings) {
        stored(context).edit { putLong("pause", settings.pauseMillis); putFloat("rate", settings.rate) }
    }
    /** The language on-device calls hear (a BCP 47 tag); empty follows the phone. */
    fun language(context: Context): String = runCatching { stored(context).getString("language", null) }.getOrNull().orEmpty()
    fun language(context: Context, tag: String) { stored(context).edit { putString("language", tag) } }
    fun choices(context: Context): VoiceChoices {
        val stored = stored(context)
        return VoiceChoices({ runCatching { stored.getString(it, null) }.getOrNull() }) { key, value ->
            stored.edit { if (value == null) remove(key) else putString(key, value) }
        }
    }
}
