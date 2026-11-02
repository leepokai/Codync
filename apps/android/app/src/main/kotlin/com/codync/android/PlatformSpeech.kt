package com.codync.android

import android.content.Context
import android.content.Intent
import android.icu.util.ULocale
import android.media.AudioFormat
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.speech.RecognitionListener
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.annotation.RequiresApi
import com.codync.android.core.*
import java.io.IOException
import java.io.OutputStream
import java.util.Locale
import kotlin.coroutines.resume
import kotlinx.coroutines.*

/** The languages Android's on-device recognizer hears on this phone: ready now, or downloadable. */
internal data class SpeechSupport(val installed: List<String> = emptyList(), val downloadable: List<String> = emptyList()) {
    val onDevice: List<String> get() = (installed + downloadable).distinct()
}

/** Android's on-device speech recognition (Android 13+, where it can be fed the call's own microphone). */
internal object PlatformSpeech {
    /** What Android hears on device; nothing before Android 13 or without an on-device recognizer. */
    suspend fun support(context: Context): SpeechSupport {
        if (Build.VERSION.SDK_INT < 33 || !SpeechRecognizer.isOnDeviceRecognitionAvailable(context)) return SpeechSupport()
        return withContext(Dispatchers.Main) {
            val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
            try {
                withTimeoutOrNull(5_000) { suspendCancellableCoroutine { done ->
                    recognizer.checkRecognitionSupport(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH), context.mainExecutor,
                        object : RecognitionSupportCallback {
                            override fun onSupportResult(support: RecognitionSupport) {
                                if (done.isActive) done.resume(SpeechSupport(support.installedOnDeviceLanguages,
                                    support.supportedOnDeviceLanguages + support.pendingOnDeviceLanguages))
                            }
                            override fun onError(error: Int) { if (done.isActive) done.resume(SpeechSupport()) }
                        })
                } } ?: SpeechSupport()
            } finally { recognizer.destroy() }
        }
    }

    /** Asks Android to fetch a language it can hear on device but hasn't downloaded yet. */
    fun download(context: Context, language: String) {
        if (Build.VERSION.SDK_INT < 33) return
        val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        try { recognizer.triggerModelDownload(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)) }
        finally { recognizer.destroy() }
    }

    /** A language's name for the settings, in the phone's language. */
    fun name(tag: String): String = ULocale.forLanguageTag(tag).getDisplayName(ULocale.getDefault()).ifEmpty { tag }
}

/** Android's recognizer asked for free-form speech in [language], the words as it hears them, on the phone where it can. */
internal fun recognizerIntent(language: String): Intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
    .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
    .putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
    .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
    .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)

/** The recognizer's best guess in [results], Chinese in the reader's script. */
internal fun recognizedText(results: Bundle?, language: String): String =
    results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()
        .let { if (VoiceLanguages.base(Locale.forLanguageTag(language)) == "zh") inReadersScript(it) else it }

/**
 * One utterance at a time through Android's on-device recognizer: the microphone's PCM is written to
 * it, so the level meter and echo cancelling are the call's own. What it hears so far is reported as
 * the stretches it has finished plus the one in progress; the call's pause decides when it's sent.
 */
@RequiresApi(33)
internal class PlatformSpeechInput(private val context: Context, private val language: String, private val mic: MicSource) {
    private var recognizer: SpeechRecognizer? = null
    private var sink: OutputStream? = null
    /** The read end, kept open until the call stops: `startListening` hands it over asynchronously. */
    private var source: ParcelFileDescriptor? = null

    suspend fun listen(generation: Long, events: (VoiceInput) -> Unit) {
        val (read, write) = ParcelFileDescriptor.createPipe()
        source = read
        val output = ParcelFileDescriptor.AutoCloseOutputStream(write).also { sink = it }
        val intent = recognizerIntent(language)
            .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, read)
            .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, 1)
            .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
            .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, Pcm.RATE)
            .putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, RecognizerIntent.EXTRA_AUDIO_SOURCE)
        withContext(Dispatchers.Main) {
            recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context).apply {
                setRecognitionListener(Listener(generation, events))
                startListening(intent)
            }
        }
        try {
            mic.start({ pcm, level ->
                events(VoiceInput.Level(generation, level))
                try { output.write(pcm) } catch (_: IOException) { /* The recognizer let go: stopListening is closing it. */ }
            }) { events(VoiceInput.Failed(generation, "Couldn't use the microphone. Check its permission and other audio apps.")) }
        } catch (error: Exception) {
            stop()
            events(VoiceInput.Failed(generation, error.message ?: "Couldn't use the microphone."))
        }
    }

    suspend fun stop() = withContext(NonCancellable) {
        mic.stop()
        val output = sink; sink = null
        try { withContext(Dispatchers.IO) { output?.close() } } catch (_: IOException) { /* Already closed by the recognizer. */ }
        withContext(Dispatchers.Main) { recognizer?.cancel(); recognizer?.destroy(); recognizer = null }
        source?.close(); source = null
    }

    private inner class Listener(private val generation: Long, private val events: (VoiceInput) -> Unit) : RecognitionListener {
        /** The stretches finished so far in this utterance. */
        private var heard = ""

        private fun text(results: Bundle?) = recognizedText(results, language)

        override fun onPartialResults(partial: Bundle?) {
            events(VoiceInput.Text(generation, VoiceLanguages.join(heard, text(partial)), false))
        }
        override fun onSegmentResults(segment: Bundle) {
            heard = VoiceLanguages.join(heard, text(segment))
            events(VoiceInput.Text(generation, heard, false))
        }
        override fun onResults(results: Bundle?) = onSegmentResults(results ?: Bundle())
        override fun onError(error: Int) {
            // Silence between stretches isn't a failure: the call's pause sends what was heard.
            if (error == SpeechRecognizer.ERROR_NO_MATCH || error == SpeechRecognizer.ERROR_SPEECH_TIMEOUT) return
            events(VoiceInput.Failed(generation, when (error) {
                SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED, SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE ->
                    "Speech recognition isn't available for this language right now."
                else -> "Couldn't hear you. Check the microphone and Android's speech settings."
            }))
        }
        override fun onEndOfSegmentedSession() { }
        override fun onReadyForSpeech(params: Bundle?) { }
        override fun onBeginningOfSpeech() { }
        override fun onRmsChanged(rmsdB: Float) { }
        override fun onBufferReceived(buffer: ByteArray?) { }
        override fun onEndOfSpeech() { }
        override fun onEvent(eventType: Int, params: Bundle?) { }
    }
}
