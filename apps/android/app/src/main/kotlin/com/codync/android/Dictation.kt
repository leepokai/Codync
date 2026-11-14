package com.codync.android

import android.content.Context
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.speech.RecognitionListener
import android.speech.SpeechRecognizer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.core.DictationText
import com.codync.android.core.MicSource
import com.codync.android.core.Pcm
import com.codync.android.core.VoiceChoices
import com.codync.android.core.VoiceProvider
import java.io.ByteArrayOutputStream
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit

/** What dictation asks of the computer and the phone (iOS `DictationService`), so a test can stand in for them. */
internal class DictationService(
    /** The engine picked beside the mic: a provider, or null for on-device speech. */
    val picked: VoiceProvider?,
    /** Whether the computer has [VoiceProvider]'s key; it throws when the computer can't be reached. */
    val hasKey: suspend (VoiceProvider) -> Boolean,
    val openMic: () -> MicSource,
    /** The computer's transcript of a recording, on the user's key. */
    val transcribe: suspend (ByteArray, VoiceProvider) -> String,
)

/** Dictation through this computer: its keys and transcription, and the phone's microphone. */
internal fun AppStore.dictationService(context: Context) = DictationService(voiceChoices().dictation,
    hasKey = { provider -> voiceStatus().also { voiceChoices().remember(it) }.configured(provider) },
    openMic = { AndroidMic(context) }, transcribe = ::transcribeDictation)

/** The recording in pieces, a few transcribed at a time and joined in order, then counted toward this month's minutes. */
private suspend fun AppStore.transcribeDictation(pcm: ByteArray, provider: VoiceProvider): String = coroutineScope {
    val model = voiceChoices().model(provider, VoiceChoices.Model.Transcribe)
    val gate = Semaphore(DictationText.IN_FLIGHT)
    val texts = DictationText.pieces(pcm.size).map { range ->
        async { gate.withPermit { voiceTranscribe(provider, model, Pcm.wav(pcm.copyOfRange(range.first, range.last + 1))).trim() } }
    }.awaitAll()
    voiceChoices().addUsage(provider, DictationText.seconds(pcm.size))
    inReadersScript(texts.filter { it.isNotEmpty() }.fold("", DictationText::join))
}

/**
 * Dictation into the message box (iOS `Dictation`). On device, Android's recognizer types the words as it hears
 * them, after what was in the box, and stops at a pause or at the recording pill's stop; it hears the call's
 * language. With a provider picked beside the mic, it records, and the pill's stop has the computer transcribe all
 * of it on the user's key; the transcript goes after what's in the box by then.
 */
internal class Dictation(private val context: Context, private val scope: CoroutineScope,
    private val clock: () -> Long = SystemClock::elapsedRealtime) {
    var listening by mutableStateOf(false); private set
    /** Recorded for a provider, waiting for its transcript. */
    var transcribing by mutableStateOf(false); private set
    /** When it started listening (`elapsedRealtime`), for the pill's time. */
    var startedAt by mutableLongStateOf(0L); private set
    /** The microphone's level while listening, 0…1. */
    var level by mutableFloatStateOf(0f); private set
    /** Why the last dictation ended early (no network or microphone), until the next. */
    var failure by mutableStateOf<String?>(null)
    /** The picked provider has no key on the computer: Voice chat settings show where to add one. */
    var needsKey by mutableStateOf(false)
    private var recognizer: SpeechRecognizer? = null
    private var mic: MicSource? = null
    /** Recording for a provider: what the microphone heard, and whose computer transcribes it. */
    private var recording: Recording? = null
    /** Changes the box's text: on-device words replace what follows `before`; a transcript goes after the box as it is then. */
    private var edit: (((String) -> String) -> Unit)? = null
    /** The box when dictation started. */
    private var before = ""
    private var transcription: Job? = null
    /** Bumped per start and stop, so a start still waiting (the key check) or a late result of an earlier one is dropped. */
    private var generation = 0

    private class Recording(val provider: VoiceProvider, val service: DictationService, val pcm: ByteArrayOutputStream = ByteArrayOutputStream())

    /** Starts with the engine picked beside the mic, after [draft]; a provider goes through [service]'s computer. */
    fun start(draft: String, service: DictationService, edit: ((String) -> String) -> Unit) {
        if (listening || transcribing) return
        failure = null; needsKey = false
        val asked = ++generation
        before = draft
        val provider = service.picked
        scope.launch { if (provider != null) record(provider, service, asked, edit) else recognize(asked, edit) }
    }

    /** The pill's stop: on-device speech has already typed its words; a recording gets transcribed. */
    fun finish() {
        val recorded = recording ?: return stop()
        val apply = edit
        stop()
        val pcm = recorded.pcm.toByteArray()
        if (pcm.size < DictationText.MIN_BYTES) return
        val asked = generation
        transcribing = true
        transcription = scope.launch {
            try {
                val said = recorded.service.transcribe(pcm, recorded.provider)
                if (asked == generation) apply?.invoke { DictationText.join(it, said) }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { if (asked == generation) failure = error.message ?: "Couldn't transcribe." }
            finally { if (asked == generation) transcribing = false }
        }
    }

    /** Stops now (another chat, a call, a send) and drops anything still on its way. */
    fun stop() {
        generation++
        transcription?.cancel(); transcription = null
        recognizer?.cancel(); recognizer?.destroy(); recognizer = null
        // Started at once: the box's scope may already be cancelled with it, and the microphone mustn't outlive the box.
        mic?.let { old -> scope.launch(start = CoroutineStart.UNDISPATCHED) { old.stop() } }; mic = null
        recording = null
        transcribing = false
        edit = null
        listening = false
        level = 0f
    }

    private fun recognize(asked: Int, edit: ((String) -> String) -> Unit) {
        if (asked != generation) return
        if (!SpeechRecognizer.isRecognitionAvailable(context)) { failure = "Speech recognition isn't available on this phone."; return }
        this.edit = edit
        startedAt = clock()
        listening = true
        // On this phone where Android can (as the iPhone does when it can), else the phone's recognizer.
        listen(asked, onDevice = Build.VERSION.SDK_INT >= 31 && SpeechRecognizer.isOnDeviceRecognitionAvailable(context))
    }

    private fun listen(asked: Int, onDevice: Boolean) {
        val language = VoicePreferences.language(context).ifEmpty { Locale.getDefault().toLanguageTag() }
        val engine = if (onDevice && Build.VERSION.SDK_INT >= 31) SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
            else SpeechRecognizer.createSpeechRecognizer(context)
        engine.setRecognitionListener(Listener(asked, language, onDevice))
        recognizer = engine
        engine.startListening(recognizerIntent(language))
    }

    /** Records for [provider] once the computer has its key, so nothing said is lost to a missing one. */
    private suspend fun record(provider: VoiceProvider, service: DictationService, asked: Int, edit: ((String) -> String) -> Unit) {
        try {
            val configured = service.hasKey(provider)
            if (asked != generation) return
            if (!configured) { needsKey = true; return }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { if (asked == generation) failure = error.message ?: "Can't reach the computer."; return }
        val microphone = service.openMic()
        val take = Recording(provider, service)
        try {
            microphone.start({ pcm, heard -> scope.launch { if (asked == generation && recording === take) { take.pcm.write(pcm); level = heard } } }) {
                scope.launch { if (asked == generation) { failure = "The microphone isn't available."; stop() } }
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { if (asked == generation) failure = "The microphone isn't available."; return }
        if (asked != generation) { microphone.stop(); return }
        mic = microphone
        this.edit = edit
        recording = take
        startedAt = clock()
        listening = true
    }

    private fun recognized(text: String, final: Boolean, asked: Int) {
        if (asked != generation) return
        val start = before
        edit?.invoke { DictationText.join(start, text) }
        if (final) stop()
    }

    private inner class Listener(private val asked: Int, private val language: String, private val onDevice: Boolean) : RecognitionListener {
        override fun onPartialResults(partial: Bundle?) { recognizedText(partial, language).takeIf { it.isNotEmpty() }?.let { recognized(it, false, asked) } }
        override fun onResults(results: Bundle?) { recognized(recognizedText(results, language), true, asked) }
        override fun onRmsChanged(rmsdB: Float) { if (asked == generation && listening) level = ((rmsdB + 2) / 12).coerceIn(0f, 1f) }
        override fun onError(error: Int) {
            if (asked != generation) return
            val noLanguage = error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED || error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE
            // The on-device recognizer may lack the language: the phone's own recognizer tries once more.
            if (onDevice && noLanguage) { recognizer?.destroy(); listen(asked, onDevice = false); return }
            // Nothing heard before the pause just ends it, as on the iPhone.
            if (error != SpeechRecognizer.ERROR_NO_MATCH && error != SpeechRecognizer.ERROR_SPEECH_TIMEOUT && error != SpeechRecognizer.ERROR_CLIENT)
                failure = when {
                    error == SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "Allow microphone access in Settings to dictate."
                    error == SpeechRecognizer.ERROR_NETWORK || error == SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "Dictation needs a connection for this language."
                    noLanguage -> "Speech recognition isn't available for this language right now."
                    else -> "Couldn't hear you. Check the microphone and Android's speech settings."
                }
            stop()
        }
        override fun onReadyForSpeech(params: Bundle?) { }
        override fun onBeginningOfSpeech() { }
        override fun onBufferReceived(buffer: ByteArray?) { }
        override fun onEndOfSpeech() { }
        override fun onEvent(eventType: Int, params: Bundle?) { }
    }
}
