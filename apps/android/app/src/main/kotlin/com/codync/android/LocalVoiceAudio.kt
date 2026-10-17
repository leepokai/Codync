package com.codync.android

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.*
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

/** Offline English recognition and an installed local TTS voice. No PCM is written to disk. */
internal class LocalVoiceAudio(private val context: Context) : VoiceAudio {
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var model: Model? = null
    private val mic = AndroidMic(context)
    /** Vosk, fed from the microphone's thread; closed only after that thread has stopped. */
    private var decoder: Recognizer? = null
    private var speech: TextToSpeech? = null
    private var completion: ((Long, String?) -> Unit)? = null
    private var speakingId: Long? = null; private var lastChunk: String? = null
    val recordingSessionId: Int? get() = mic.recordingSessionId

    override suspend fun initialize(): String {
        require(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        withContext(Dispatchers.IO) {
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
        val voices = engine.voices.orEmpty().filter { !it.isNetworkConnectionRequired && it.locale.language == "en" }
        val voice = voices.maxWithOrNull(compareBy<android.speech.tts.Voice> { it.locale == Locale.US }.thenBy { it.quality })
            ?: throw VoiceUnavailable("Install an offline English voice in Android's text-to-speech settings.")
        check(engine.setVoice(voice) == TextToSpeech.SUCCESS)
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
        return "Offline English · Vosk"
    }
    override suspend fun listen(generation: Long, events: (VoiceInput) -> Unit) {
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
        withContext(NonCancellable) { mic.stop(); decoder?.close(); decoder = null }
    }
    override suspend fun speak(id: Long, text: String, rate: Float, completed: (Long, String?) -> Unit) {
        val engine = requireNotNull(speech)
        val chunks = SpokenText.chunks(text)
        speakingId = id; completion = completed; lastChunk = "codync-$id-${chunks.lastIndex}"
        check(engine.setSpeechRate(rate) == TextToSpeech.SUCCESS)
        for ((index, chunk) in chunks.withIndex()) {
            check(engine.speak(chunk, TextToSpeech.QUEUE_ADD, Bundle(), "codync-$id-$index") == TextToSpeech.SUCCESS)
        }
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

/** Call settings on this phone: the pause and reading speed, and the cloud engine choices (`VoiceChoices`). */
internal object VoicePreferences {
    private fun stored(context: Context) = context.getSharedPreferences("codync-voice", Context.MODE_PRIVATE)
    fun load(context: Context): VoiceSettings {
        val stored = stored(context)
        return runCatching { VoiceSettings(stored.getLong("pause", 1_500), stored.getFloat("rate", 1f)) }.getOrDefault(VoiceSettings())
    }
    fun save(context: Context, settings: VoiceSettings) {
        stored(context).edit { putLong("pause", settings.pauseMillis); putFloat("rate", settings.rate) }
    }
    fun choices(context: Context): VoiceChoices {
        val stored = stored(context)
        return VoiceChoices({ runCatching { stored.getString(it, null) }.getOrNull() }) { key, value ->
            stored.edit { if (value == null) remove(key) else putString(key, value) }
        }
    }
}
