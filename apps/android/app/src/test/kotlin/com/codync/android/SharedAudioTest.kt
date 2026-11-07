package com.codync.android

import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.*
import org.junit.Test

/** A voice note's player (iOS `SharedAudioTests`): its fetch from the computer is the test's to answer. */
class SharedAudioTest {
    private class Speaker : Sound {
        val log = mutableListOf<String>()
        var startable = true
        var ended: ((Boolean) -> Unit)? = null
        override var playing = false
        override val duration = 48.0
        override var position = 0.0
        override fun start(): Boolean {
            log += "start"
            playing = startable
            return startable
        }
        override fun pause() { log += "pause"; playing = false }
        override fun seek(seconds: Double) { log += "seek $seconds"; position = seconds }
        override fun release() { log += "release"; playing = false }
    }

    private val speaker = Speaker()
    private var fetch = CompletableDeferred<File>()
    private var loads = 0
    private val audio = SharedAudio(load = { loads++; fetch.await() },
        open = { _, stopped -> speaker.ended = stopped; speaker.log += "open"; speaker })

    @Test fun `a second tap while the sound loads plays it once`() = runBlocking {
        val first = launch { audio.toggle() }
        yield()
        audio.toggle()
        assertEquals(AudioPhase.Loading, audio.phase)
        assertEquals(1, loads)
        fetch.complete(File("note.m4a"))
        first.join()
        assertEquals(listOf("open", "start"), speaker.log)
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(48.0, audio.duration, 0.0)
        audio.toggle()
        assertEquals(AudioPhase.Paused, audio.phase)
    }

    @Test fun `a card released while its sound loads plays nothing`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        audio.release()
        fetch.complete(File("note.m4a"))
        tap.join()
        assertEquals(emptyList<String>(), speaker.log)
        assertEquals(AudioPhase.Loading, audio.phase)
    }

    @Test fun `a sound that fails to play fails the card and the next tap loads again`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.completeExceptionally(java.io.IOException("gone"))
        tap.join()
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(1, loads)

        fetch = CompletableDeferred()
        val retry = launch { audio.toggle() }
        yield()
        assertEquals(AudioPhase.Loading, audio.phase)
        assertEquals(2, loads)
        fetch.complete(File("note.m4a"))
        retry.join()
        assertEquals(listOf("open", "start"), speaker.log)
        assertEquals(AudioPhase.Playing, audio.phase)
    }

    @Test fun `a sound that can't start fails the card`() = runBlocking {
        speaker.startable = false
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(listOf("open", "start", "release"), speaker.log)
    }

    @Test fun `playing to the end goes back to a replayable start`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        speaker.playing = true
        speaker.position = 48.0
        speaker.ended!!(true)
        assertEquals(AudioPhase.Idle, audio.phase)
        assertEquals(0.0, audio.elapsed, 0.0)
        assertEquals(48.0, audio.duration, 0.0)
        // A tap replays the kept sound from the start; nothing loads again.
        audio.toggle()
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(1, loads)
    }

    @Test fun `losing the audio focus pauses where the sound was`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        speaker.playing = false
        speaker.position = 7.0
        speaker.ended!!(false)
        assertEquals(AudioPhase.Paused, audio.phase)
        assertEquals(7.0, audio.elapsed, 0.0)
    }

    @Test fun `seeking holds the playhead to the sound and keeps it playing or paused`() = runBlocking {
        audio.seek(3.0)
        assertTrue("nothing to seek before the sound loads", speaker.log.isEmpty())
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        audio.seek(20.0)
        audio.seek(-5.0)
        audio.seek(60.0)
        assertEquals(listOf("seek 20.0", "seek 0.0", "seek 48.0"), speaker.log.drop(2))
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(48.0, audio.elapsed, 0.0)
        audio.toggle()
        audio.seek(12.0)
        assertEquals(AudioPhase.Paused, audio.phase)
        assertEquals(12.0, audio.elapsed, 0.0)
        // Playing again picks up where it was moved to.
        audio.toggle()
        assertEquals(12.0, audio.elapsed, 0.0)
    }
}
