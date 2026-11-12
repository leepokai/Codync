package com.codync.android

import com.codync.android.core.SharedFile
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** The caption under a shared file's name, and the size format every card shares. */
class FileCardCaptionTest {
    private val file = SharedFile("f1", "note.m4a", 380_030, "0".repeat(64))
    private class Speaker(override var position: Double = 0.0) : Sound {
        override var playing = false
        override val duration = 48.0
        override fun start(): Boolean { playing = true; return true }
        override fun pause() { playing = false }
        override fun seek(seconds: Double) { position = seconds }
        override fun release() {}
    }

    /** A card whose sound loaded and played `elapsed` seconds. */
    private fun loaded(elapsed: Double): SharedAudio {
        val speaker = Speaker()
        val audio = SharedAudio(load = { File("note.m4a") }, open = { _, _ -> speaker })
        runBlocking { audio.toggle() }
        speaker.position = elapsed
        audio.syncElapsed()
        return audio
    }

    private fun failed(): SharedAudio {
        val audio = SharedAudio(load = { throw java.io.IOException("gone") }, open = { _, _ -> Speaker() })
        runBlocking { audio.toggle() }
        return audio
    }

    @Test fun `a player failure outranks the download and the progress`() {
        assertEquals("Couldn't play. Tap to retry.",
            detail(file, FileDownloadState.Downloading(1_000), failed()))
        assertEquals("Couldn't play. Tap to retry.", detail(file, null, failed()))
    }

    @Test fun `an ongoing download outranks the sound's progress`() {
        assertEquals("1 KB of 380 KB", detail(file, FileDownloadState.Downloading(1_000), loaded(7.0)))
    }

    @Test fun `a failed download keeps its message`() {
        assertEquals("Couldn't download the file. Tap to retry.",
            detail(file, FileDownloadState.Failed("Couldn't download the file."), loaded(7.0)))
    }

    @Test fun `a loaded sound shows elapsed over duration`() {
        assertEquals("0:07 / 0:48", detail(file, null, loaded(7.0)))
        assertEquals("0:07 / 0:48 · Downloaded", detail(file, FileDownloadState.Saved, loaded(7.0)))
        assertEquals("380 KB", detail(file, null))
        assertEquals("380 KB · Downloaded", detail(file, FileDownloadState.Saved))
    }

    @Test fun `an hour of sound shows hours`() {
        assertEquals("1:00:07", clock(3607.0))
        assertEquals("0:07", clock(7.49))
        assertEquals("0:08", clock(7.5))
    }

    @Test fun `an accessibility adjust moves five seconds whatever value it asks for`() {
        assertEquals(25.0, seekStepTarget(20.0, 48f.compareTo(20f)), 0.0)
        assertEquals(15.0, seekStepTarget(20.0, 0f.compareTo(20f)), 0.0)
        assertEquals(20.0, seekStepTarget(20.0, 20f.compareTo(20f)), 0.0)
    }

    @Test fun `sizes count in thousands as ByteCountFormatter`() {
        val cases = mapOf(
            0L to "Zero KB", 1L to "1 byte", 43L to "43 bytes", 999L to "999 bytes",
            1_000L to "1 KB", 1_499L to "1 KB", 1_500L to "2 KB", 47_600L to "48 KB",
            64_400L to "64 KB", 380_030L to "380 KB", 999_500L to "1 MB", 1_049_999L to "1 MB",
            1_050_000L to "1.1 MB", 1_234_567L to "1.2 MB", 12_345_678L to "12.3 MB",
            1_000_000_000L to "1 GB", 1_234_567_890L to "1.23 GB")
        for ((size, shown) in cases) assertEquals("$size", shown, formatBytes(size))
    }
}
