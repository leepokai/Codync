package com.codync.android

import android.content.Context
import android.media.AudioAttributes
import android.speech.tts.TextToSpeech
import java.util.Locale
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Reads one message aloud with Android's text-to-speech, at the voice settings' speed and the voice's
 * normal pitch, as the desktop and the phone do. Only one message reads at a time; starting another
 * (or closing the chat) stops the first.
 */
internal class MessageSpeech(context: Context) {
    private val app = context.applicationContext
    private var engine: TextToSpeech? = null
    private var ready = false
    private val _reading = MutableStateFlow<String?>(null)
    val reading = _reading.asStateFlow()

    private fun start(): TextToSpeech? {
        engine?.let { if (ready) return it }
        val created = TextToSpeech(app) { status ->
            if (status != TextToSpeech.SUCCESS) return@TextToSpeech
            ready = true
            engine?.apply {
                language = Locale.getDefault()
                setPitch(1f)
                setSpeechRate(VoicePreferences.load(app).rate)
                setAudioAttributes(AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            }
            pending?.let { (id, text) -> pending = null; speak(id, text) }
        }
        engine = created
        return null
    }

    fun speak(entryId: String, text: String) {
        _reading.value = entryId
        val active = start()
        if (active == null || !ready) {
            // The engine's first start is asynchronous: speak once it says it's ready.
            engine?.let { pending = entryId to text }
            return
        }
        active.setSpeechRate(VoicePreferences.load(app).rate)
        active.speak(text, TextToSpeech.QUEUE_FLUSH, null, "read-$entryId")
    }

    private var pending: Pair<String, String>? = null

    /** Applies a new speed to the next (or running) reading. */
    fun setRate(rate: Float) { engine?.setSpeechRate(rate) }

    fun stop() {
        _reading.value = null
        pending = null
        engine?.stop()
    }

    fun shutdown() {
        stop()
        engine?.shutdown()
        engine = null
        ready = false
    }
}
