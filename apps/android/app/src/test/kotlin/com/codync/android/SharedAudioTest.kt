package com.codync.android

import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
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
        var stopped: ((SoundStop) -> Unit)? = null
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
    /** The card's scope: unconfined, so a fetch starts the moment it's asked for. */
    private val card = CoroutineScope(Dispatchers.Unconfined)
    private val audio = SharedAudio(load = { loads++; fetch.await() },
        open = { _, stopped -> speaker.stopped = stopped; speaker.log += "open"; speaker }, scope = card,
        length = { 9.4 })

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

    @Test fun `a card released while its sound prepares releases it without playing`() = runBlocking {
        val opening = CompletableDeferred<Unit>()
        val prepared = CompletableDeferred<Sound>()
        val audio = SharedAudio(load = { File("note.m4a") }, open = { _, _ ->
            opening.complete(Unit)
            prepared.await()
        }, scope = card)
        val tap = launch { audio.toggle() }
        opening.await()
        audio.release()
        prepared.complete(speaker)
        tap.join()
        assertEquals(listOf("release"), speaker.log)
        assertFalse(speaker.playing)
        assertEquals(0.0, audio.duration, 0.0)
        audio.release()
        assertEquals(listOf("release"), speaker.log)
    }

    @Test fun `a sound that fails to play fails the card and the next tap loads again`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.completeExceptionally(java.io.IOException("gone"))
        tap.join()
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(VoiceNoteFailure.Load, audio.failure)
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

    @Test fun `a tap whose scope goes while the sound loads leaves the next tap to play the same fetch`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        tap.cancelAndJoin()
        assertEquals(AudioPhase.Idle, audio.phase)

        val retry = launch { audio.toggle() }
        yield()
        assertEquals("the fetch runs on in the card's scope", 1, loads)
        fetch.complete(File("note.m4a"))
        retry.join()
        assertEquals(AudioPhase.Playing, audio.phase)
    }

    @Test fun `a sound that can't start fails the card`() = runBlocking {
        speaker.startable = false
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(VoiceNoteFailure.Playback, audio.failure)
        assertEquals(listOf("open", "start", "release"), speaker.log)
    }

    @Test fun `playing to the end goes back to a replayable start`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        speaker.playing = true
        speaker.position = 48.0
        speaker.stopped!!(SoundStop.Ended)
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
        speaker.stopped!!(SoundStop.Interrupted)
        assertEquals(AudioPhase.Paused, audio.phase)
        assertEquals(7.0, audio.elapsed, 0.0)
    }

    @Test fun `a sound that breaks while playing fails the card and the next tap loads again`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.complete(File("note.m4a"))
        tap.join()
        speaker.stopped!!(SoundStop.Broke)
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(0.0, audio.duration, 0.0)
        assertEquals(listOf("open", "start", "release"), speaker.log)

        val retry = launch { audio.toggle() }
        yield()
        assertEquals(2, loads)
        retry.cancelAndJoin()
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

    @Test fun `a seek before the first play starts the sound there`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        fetch.complete(File("note.m4a"))
        preloading.join()
        assertEquals(9.4, audio.duration, 0.0)

        audio.seek(5.0)
        assertEquals(5.0, audio.elapsed, 0.0)
        assertEquals("no sound opens before the tap", emptyList<String>(), speaker.log)
        audio.toggle()
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(listOf("open", "seek 5.0", "start"), speaker.log)
        assertEquals(5.0, audio.elapsed, 0.0)
    }

    @Test fun `a preload has the next tap play the bytes it already fetched`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        fetch.complete(File("note.m4a"))
        preloading.join()
        assertEquals(AudioPhase.Idle, audio.phase)
        assertEquals(9.4, audio.duration, 0.0)
        assertEquals(emptyList<String>(), speaker.log)

        audio.toggle()
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(listOf("open", "start"), speaker.log)
        assertEquals(1, loads)
    }

    @Test fun `a tap during a preload plays the same fetch when it lands`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        val tap = launch { audio.toggle() }
        yield()
        assertEquals(AudioPhase.Loading, audio.phase)
        fetch.complete(File("note.m4a"))
        preloading.join()
        tap.join()
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(listOf("open", "start"), speaker.log)
        assertEquals(1, loads)
    }

    @Test fun `a tap waiting on a preload that restarts still plays the one fetch`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        val tap = launch { audio.toggle() }
        yield()
        // The card's connection changed: its preload effect is cancelled and starts again.
        preloading.cancelAndJoin()
        val restarted = launch { audio.preload() }
        fetch.complete(File("note.m4a"))
        tap.join()
        restarted.join()
        assertEquals(AudioPhase.Playing, audio.phase)
        assertEquals(listOf("open", "start"), speaker.log)
        assertEquals(1, loads)
    }

    @Test fun `a failed preload leaves the tap to fetch and report`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        fetch.completeExceptionally(java.io.IOException("gone"))
        preloading.join()
        assertEquals(AudioPhase.Idle, audio.phase)
        assertEquals(0.0, audio.duration, 0.0)

        fetch = CompletableDeferred()
        val tap = launch { audio.toggle() }
        yield()
        assertEquals(2, loads)
        fetch.complete(File("note.m4a"))
        tap.join()
        assertEquals(AudioPhase.Playing, audio.phase)
    }

    @Test fun `a card released while its sound preloads plays nothing`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        audio.release()
        fetch.complete(File("note.m4a"))
        preloading.join()
        assertEquals(emptyList<String>(), speaker.log)
        assertEquals(0.0, audio.duration, 0.0)
        assertEquals(AudioPhase.Idle, audio.phase)
    }

    @Test fun `a sound that fails to play after a preload keeps the length the preload read`() = runBlocking {
        val preloading = launch { audio.preload() }
        yield()
        fetch.complete(File("note.m4a"))
        preloading.join()
        assertEquals(9.4, audio.duration, 0.0)

        speaker.startable = false
        audio.toggle()
        assertEquals(AudioPhase.Failed, audio.phase)
        assertEquals(9.4, audio.duration, 0.0)
    }
}
