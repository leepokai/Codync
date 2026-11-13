package com.codync.android

import android.content.ContextWrapper
import com.codync.android.core.DictationEngine
import com.codync.android.core.DictationText
import com.codync.android.core.MicSource
import com.codync.android.core.VoiceProvider
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.*
import org.junit.Test

/** Dictation through a provider (iOS `DictationTests`): the computer's answers and the microphone are the test's. */
class DictationTest {
    /** The phone's microphone, hearing what the test plays it. */
    private class Mic : MicSource {
        var heard: ((ByteArray, Float) -> Unit)? = null
        var stopped = false
        override suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit) { heard = frames }
        override suspend fun stop() { stopped = true }
    }

    /** A call to the computer that waits for the test's answer. */
    private class Pending<T> {
        val answer = CompletableDeferred<T>()
        var calls = 0; private set
        suspend fun call(): T { calls++; return answer.await() }
    }

    /** Dictation with an engine picked, on a scope the test cancels when it's done. */
    private fun dictating(engine: DictationEngine = DictationEngine.Cloud(VoiceProvider.OpenAI), gate: Pending<DictationGate>? = null,
        transcript: Pending<String> = Pending(), mic: Mic = Mic(), seen: MutableList<DictationEngine>? = null,
        body: suspend (Dictation, DictationService, CoroutineScope) -> Unit) = runBlocking {
        val scope = CoroutineScope(coroutineContext + Job(coroutineContext[Job]))
        val service = DictationService(engine, { gate?.call() ?: DictationGate.Open }, { mic }) { _, picked -> seen?.add(picked); transcript.call() }
        try { body(Dictation(object : ContextWrapper(null) {}, scope, clock = { 0L }), service, scope) } finally { scope.cancel() }
    }

    private suspend fun until(check: () -> Boolean): Boolean {
        repeat(1_000) { if (check()) return true; yield() }
        return check()
    }

    /** Records [bytes] of what [mic] hears and taps the pill's stop; the transcript changes the box through [edit]. */
    private suspend fun Dictation.dictate(bytes: Int, mic: Mic, service: DictationService, edit: ((String) -> String) -> Unit) {
        start("", service, edit)
        assertTrue(until { listening })
        mic.heard?.invoke(ByteArray(bytes), 0.5f)
        assertTrue(until { level > 0 })
        finish()
    }

    @Test fun `a recording is transcribed after what the box holds then`() {
        val transcript = Pending<String>(); val mic = Mic()
        dictating(transcript = transcript, mic = mic) { dictation, service, _ ->
            var box = "Fix the bug"
            dictation.dictate(DictationText.MIN_BYTES, mic, service) { box = it(box) }
            assertTrue(dictation.transcribing && !dictation.listening && mic.stopped)
            assertTrue(until { transcript.calls == 1 })
            box += " now"
            transcript.answer.complete("in the parser")
            assertTrue(until { !dictation.transcribing })
            assertEquals("Fix the bug now in the parser", box)
            assertNull(dictation.failure)
        }
    }

    @Test fun `a transcript arriving after a stop is dropped`() {
        val transcript = Pending<String>(); val mic = Mic()
        dictating(transcript = transcript, mic = mic) { dictation, service, _ ->
            var box = "Fix the bug"
            dictation.dictate(DictationText.MIN_BYTES, mic, service) { box = it(box) }
            assertTrue(until { transcript.calls == 1 })
            dictation.stop()
            assertFalse(dictation.transcribing)
            transcript.answer.complete("too late")
            repeat(10) { yield() }
            assertEquals("Fix the bug", box)
            assertTrue(dictation.failure == null && !dictation.transcribing)
        }
    }

    @Test fun `a failed transcription says why`() {
        val transcript = Pending<String>(); val mic = Mic()
        dictating(transcript = transcript, mic = mic) { dictation, service, _ ->
            var box = "Fix the bug"
            dictation.dictate(DictationText.MIN_BYTES, mic, service) { box = it(box) }
            assertTrue(until { transcript.calls == 1 })
            transcript.answer.completeExceptionally(IOException("The computer is offline."))
            assertTrue(until { !dictation.transcribing })
            assertEquals("The computer is offline.", dictation.failure)
            assertEquals("Fix the bug", box)
        }
    }

    @Test fun `a tap of the mic sends nothing`() {
        val transcript = Pending<String>(); val mic = Mic()
        dictating(transcript = transcript, mic = mic) { dictation, service, _ ->
            dictation.dictate(DictationText.MIN_BYTES - 1, mic, service) { }
            repeat(10) { yield() }
            assertFalse(dictation.transcribing)
            assertEquals(0, transcript.calls)
        }
    }

    @Test fun `without the key it asks for settings and never opens the microphone`() {
        val gate = Pending<DictationGate>(); val mic = Mic()
        dictating(gate = gate, mic = mic) { dictation, service, _ ->
            dictation.start("", service) { }
            assertTrue(until { gate.calls == 1 })
            gate.answer.complete(DictationGate.NeedsKey)
            assertTrue(until { dictation.needsKey })
            assertTrue(dictation.failure == null && !dictation.listening && mic.heard == null)
        }
    }

    @Test fun `this computer without its speech model says why and never opens the microphone`() {
        val gate = Pending<DictationGate>(); val mic = Mic()
        dictating(engine = DictationEngine.Local, gate = gate, mic = mic) { dictation, service, _ ->
            dictation.start("", service) { }
            assertTrue(until { gate.calls == 1 })
            gate.answer.complete(DictationGate.Blocked("Link a speech server in Codync on your computer."))
            assertTrue(until { dictation.failure != null })
            assertEquals("Link a speech server in Codync on your computer.", dictation.failure)
            assertTrue(!dictation.needsKey && !dictation.listening && mic.heard == null)
        }
    }

    @Test fun `a recording on this computer is transcribed by it`() {
        val transcript = Pending<String>(); val mic = Mic(); val seen = mutableListOf<DictationEngine>()
        dictating(engine = DictationEngine.Local, transcript = transcript, mic = mic, seen = seen) { dictation, service, _ ->
            var box = "Fix the bug"
            dictation.dictate(DictationText.MIN_BYTES, mic, service) { box = it(box) }
            assertTrue(until { transcript.calls == 1 })
            transcript.answer.complete("in the parser")
            assertTrue(until { !dictation.transcribing })
            assertEquals(listOf<DictationEngine>(DictationEngine.Local), seen)
            assertEquals("Fix the bug in the parser", box)
        }
    }

    @Test fun `an unreachable computer says why`() {
        val gate = Pending<DictationGate>(); val mic = Mic()
        dictating(gate = gate, mic = mic) { dictation, service, _ ->
            dictation.start("", service) { }
            assertTrue(until { gate.calls == 1 })
            gate.answer.completeExceptionally(IOException("The computer is offline."))
            assertTrue(until { dictation.failure != null })
            assertEquals("The computer is offline.", dictation.failure)
            assertTrue(!dictation.listening && mic.heard == null)
        }
    }

    @Test fun `a stop while the key is checked wins`() {
        for (gate in listOf(DictationGate.Open, DictationGate.NeedsKey)) {
            val key = Pending<DictationGate>(); val mic = Mic()
            dictating(gate = key, mic = mic) { dictation, service, _ ->
                dictation.start("", service) { }
                assertTrue(until { key.calls == 1 })
                dictation.stop()
                if (gate == DictationGate.Open) key.answer.complete(gate) else key.answer.completeExceptionally(IOException("Offline"))
                repeat(10) { yield() }
                assertTrue(!dictation.listening && mic.heard == null && dictation.failure == null && !dictation.needsKey)
            }
        }
    }

    @Test fun `the microphone stops with the box even after the box's scope is gone`() {
        val mic = Mic()
        dictating(mic = mic) { dictation, service, scope ->
            dictation.start("", service) { }
            assertTrue(until { dictation.listening })
            scope.cancel()
            dictation.stop()
            assertTrue(mic.stopped)
        }
    }
}
