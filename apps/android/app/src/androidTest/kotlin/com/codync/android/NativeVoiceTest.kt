package com.codync.android

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.security.MessageDigest
import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.vosk.LibVosk
import org.vosk.Model
import org.vosk.Recognizer

class NativeVoiceTest {
    @Test fun theBundledLocalModelRecognizesThePinnedSpeechFixture() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val wav = instrumentation.context.assets.open("voice-test.wav").use { it.readBytes() }
        val digest = MessageDigest.getInstance("SHA-256").digest(wav).joinToString("") { "%02x".format(it) }
        assertEquals("dcfea5712c43a43ba7ae8083afb39d36993e5a69c46e88b68aaa72b65cb615bb", digest)
        val header = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        var offset = 12; var pcm: ByteArray? = null
        while (offset + 8 <= wav.size) {
            val kind = String(wav, offset, 4, Charsets.US_ASCII); val size = header.getInt(offset + 4)
            require(size >= 0 && size <= wav.size - offset - 8)
            if (kind == "fmt ") {
                assertEquals(1, header.getShort(offset + 8).toInt()); assertEquals(1, header.getShort(offset + 10).toInt())
                assertEquals(16_000, header.getInt(offset + 12)); assertEquals(16, header.getShort(offset + 22).toInt())
            }
            if (kind == "data") pcm = wav.copyOfRange(offset + 8, offset + 8 + size)
            offset += 8 + size + size % 2
        }
        val samples = requireNotNull(pcm)
        val text = withContext(Dispatchers.IO) {
            LibVosk.vosk_set_log_level(-1)
            val directory = VoskModels.english(instrumentation.targetContext)
            Model(directory.absolutePath).use { model -> Recognizer(model, 16_000f).use { recognizer ->
                val results = mutableListOf<String>(); var position = 0
                while (position < samples.size) {
                    val count = minOf(4_096, samples.size - position)
                    val chunk = samples.copyOfRange(position, position + count)
                    if (recognizer.acceptWaveForm(chunk, chunk.size)) results.add(JSONObject(recognizer.result).optString("text"))
                    position += count
                }
                results.add(JSONObject(recognizer.finalResult).optString("text")); results.joinToString(" ")
            } }
        }
        assertTrue("The model must recognize the fixture's first spoken digits", text.contains("one zero zero zero one"))
        assertTrue("The model must recognize the fixture's final spoken digits", text.contains("zero one eight zero three"))
    }
    @Test fun nativeMicrophoneCaptureStopsAndOfflinePlaybackCompletes() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("voiceAudio") == "true")
        assertTrue("Microphone fixtures belong on emulators", Build.PRODUCT.contains("sdk"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals(PackageManager.PERMISSION_GRANTED, ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO))
        ActivityScenario.launch(MainActivity::class.java).use {
            val engine = LocalVoiceAudio(context, "")
            try { runBlocking { withContext(Dispatchers.Main) {
                val name = engine.initialize(); assertTrue(name, name.endsWith(" · Vosk") || name.endsWith(" · Android speech"))
                val capturing = CompletableDeferred<Unit>()
                engine.listen(1) { event -> if (event is VoiceInput.Level) capturing.complete(Unit) }
                withTimeout(10_000) { capturing.await() }
                assertNotNull(engine.recordingSessionId)
                engine.stopListening(); assertNull(engine.recordingSessionId)
                val spoken = CompletableDeferred<String?>()
                engine.speak(1, "This is the offline voice acceptance fixture.", 1f) { _, error -> spoken.complete(error) }
                val error = withTimeout(30_000) { spoken.await() }
                assertNull(error)
                engine.stopSpeaking()
            } } } finally { runBlocking { withContext(Dispatchers.Main + NonCancellable) { engine.close() } } }
        }
    }
}
