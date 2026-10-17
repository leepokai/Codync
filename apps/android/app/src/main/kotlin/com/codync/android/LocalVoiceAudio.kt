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
import kotlin.math.log10

/** Offline English recognition and an installed local TTS voice. No PCM is written to disk. */
internal class LocalVoiceAudio(private val context: Context) : VoiceAudio {
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var model: Model? = null
    @Volatile private var recorder: AudioRecord? = null
    private var recording: Job? = null
    private var speech: TextToSpeech? = null
    private var completion: ((Long, String?) -> Unit)? = null
    private var speakingId: Long? = null; private var lastChunk: String? = null
    val recordingSessionId: Int? get() = recorder?.audioSessionId

    override suspend fun initialize(): String {
        require(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        withContext(Dispatchers.IO) {
            try {
                LibVosk.vosk_set_log_level(-1)
                val directory = VoiceModels.english(context)
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
        check(recording == null)
        require(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        val installed = requireNotNull(model)
        recording = worker.launch {
            var microphone: AudioRecord? = null; var decoder: Recognizer? = null
            try {
                val format = AudioFormat.Builder().setSampleRate(16_000).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build()
                val minimum = AudioRecord.getMinBufferSize(16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
                check(minimum > 0)
                microphone = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
                    .setAudioFormat(format).setBufferSizeInBytes(maxOf(minimum, 12_800)).build()
                recorder = microphone
                check(microphone.state == AudioRecord.STATE_INITIALIZED)
                decoder = Recognizer(installed, 16_000f)
                ensureActive(); microphone.startRecording()
                check(microphone.recordingState == AudioRecord.RECORDSTATE_RECORDING)
                val buffer = ShortArray(3_200); var previous = ""
                while (isActive) {
                    val count = microphone.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
                    if (!isActive) break
                    if (count < 0) throw IOException("Microphone read failed")
                    if (count == 0) { delay(10); continue }
                    var power = 0.0
                    for (i in 0 until count) { val sample = buffer[i].toDouble() / 32_768; power += sample * sample }
                    val db = 10 * log10(maxOf(power / count, 1e-10))
                    events(VoiceInput.Level(generation, ((db + 50) / 50).toFloat().coerceIn(0f, 1f)))
                    if (decoder.acceptWaveForm(buffer, count)) {
                        val text = JSONObject(decoder.result).optString("text")
                        previous = ""; if (text.isNotBlank()) events(VoiceInput.Text(generation, text, true))
                    } else {
                        val text = JSONObject(decoder.partialResult).optString("partial")
                        if (text != previous) { previous = text; events(VoiceInput.Text(generation, text, false)) }
                    }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { if (isActive) events(VoiceInput.Failed(generation, "Couldn't use the microphone. Check its permission and other audio apps.")) }
            finally {
                runCatching { microphone?.stop() }; runCatching { microphone?.release() }
                if (recorder === microphone) recorder = null
                decoder?.close()
            }
        }
    }
    override suspend fun stopListening() {
        withContext(NonCancellable) {
            val old = recording; recording = null
            old?.cancel(); runCatching { recorder?.stop() }; old?.join()
        }
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
            worker.cancel()
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
internal object VoiceModels {
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

internal object VoicePreferences {
    fun load(context: Context): VoiceSettings {
        val stored = context.getSharedPreferences("codync-voice", Context.MODE_PRIVATE)
        return runCatching { VoiceSettings(stored.getLong("pause", 1_500), stored.getFloat("rate", 1f)) }.getOrDefault(VoiceSettings())
    }
    fun save(context: Context, settings: VoiceSettings) {
        context.getSharedPreferences("codync-voice", Context.MODE_PRIVATE).edit()
            .putLong("pause", settings.pauseMillis).putFloat("rate", settings.rate).apply()
    }
}
