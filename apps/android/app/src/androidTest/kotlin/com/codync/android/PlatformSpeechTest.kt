package com.codync.android

import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Android's on-device recognizer hears the pinned speech fixture through the call's own audio path. */
class PlatformSpeechTest {
    /** Plays the fixture's PCM in real time, as the microphone would. */
    private class FixtureMic(private val pcm: ByteArray) : MicSource {
        private var playing: Job? = null
        override suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit) {
            playing = CoroutineScope(Dispatchers.IO).launch {
                // Silence after the speech lets the recognizer finish its last stretch.
                val audio = pcm + ByteArray(Pcm.RATE * 2 * 2)
                for (start in audio.indices step 3_200) {
                    frames(audio.copyOfRange(start, minOf(start + 3_200, audio.size)), 0.5f)
                    delay(100)
                }
            }
        }
        override suspend fun stop() { playing?.cancelAndJoin() }
    }

    @Test fun androidHearsTheSpeechFixtureOnDevice() = runBlocking {
        assumeTrue(Build.VERSION.SDK_INT >= 33)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val support = PlatformSpeech.support(context)
        assumeTrue("No installed on-device English: $support", "en-US" in support.installed)
        val wav = instrumentation.context.assets.open("voice-test.wav").use { it.readBytes() }
        val input = PlatformSpeechInput(context, "en-US", FixtureMic(samples(wav)))
        val heard = Channel<String>(Channel.UNLIMITED)
        val failure = CompletableDeferred<String>()
        input.listen(1) { event ->
            when (event) {
                is VoiceInput.Text -> heard.trySend(event.value)
                is VoiceInput.Failed -> failure.complete(event.message)
                is VoiceInput.Level -> { }
            }
        }
        var last = ""
        try {
            withTimeout(60_000) {
                while (!last.contains("one") && !last.contains("1")) {
                    select(heard, failure).let { if (it.second != null) fail(it.second) else last = it.first }
                }
            }
        } finally { input.stop() }
        assertTrue(last, last.isNotBlank())
    }

    /** The fixture's 16 kHz mono PCM16 samples: its `data` chunk. */
    private fun samples(wav: ByteArray): ByteArray {
        val header = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        var offset = 12
        while (offset + 8 <= wav.size) {
            val size = header.getInt(offset + 4)
            if (String(wav, offset, 4, Charsets.US_ASCII) == "data") return wav.copyOfRange(offset + 8, offset + 8 + size)
            offset += 8 + size + size % 2
        }
        error("The fixture has no audio")
    }

    private suspend fun select(heard: Channel<String>, failure: CompletableDeferred<String>): Pair<String, String?> =
        kotlinx.coroutines.selects.select {
            heard.onReceive { it to null }
            failure.onAwait { "" to it }
        }
}
