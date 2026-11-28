package com.codync.android

import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Android's on-device recognizer hears the pinned speech fixture through the call's own audio path. */
class PlatformSpeechTest {
    @Test fun androidHearsTheSpeechFixtureOnDevice() = runBlocking {
        assumeTrue(Build.VERSION.SDK_INT >= 33)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val support = PlatformSpeech.support(context)
        assumeTrue("No installed on-device English: $support", "en-US" in support.installed)
        val wav = instrumentation.context.assets.open("voice-test.wav").use { it.readBytes() }
        // Silence after the speech lets the recognizer finish its last stretch.
        val input = PlatformSpeechInput(context, "en-US", ClipMic(ClipMic.samples(wav) + ByteArray(Pcm.RATE * 2 * 2)))
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

    private suspend fun select(heard: Channel<String>, failure: CompletableDeferred<String>): Pair<String, String?> =
        kotlinx.coroutines.selects.select {
            heard.onReceive { it to null }
            failure.onAwait { "" to it }
        }
}
