package com.codync.android

import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import java.io.File
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.suspendCancellableCoroutine

/** The state of a voice note's player, shown by the card's play button and caption. */
enum class AudioPhase { Idle, Loading, Playing, Paused, Failed }

/** Why a sound stopped on its own: played to its end, lost the audio focus, or broke while decoding. */
internal enum class SoundStop { Ended, Interrupted, Broke }

/** A loaded sound, ready to play (`MediaSound` in the app). */
internal interface Sound {
    val playing: Boolean
    /** Seconds long. */
    val duration: Double
    /** Seconds played so far. */
    val position: Double
    /** Whether it started (another app may hold the audio focus). */
    fun start(): Boolean
    fun pause()
    /** Moves its playhead (seconds), playing or paused. */
    fun seek(seconds: Double)
    fun release()
}

/**
 * Plays a sound a bot shared (a voice note), fetched once from the computer by [load] (iOS `SharedAudio`); one card's
 * player. [open] prepares the player and calls back when it stops on its own.
 */
internal class SharedAudio(
    private val load: suspend () -> File,
    private val open: suspend (File, stopped: (SoundStop) -> Unit) -> Sound,
) {
    var phase by mutableStateOf(AudioPhase.Idle)
        private set
    /** Seconds long, known once the sound loads. */
    var duration by mutableStateOf(0.0)
        private set
    /** Seconds played so far; the card's progress ticker updates it while it plays. */
    var elapsed by mutableStateOf(0.0)
        private set
    private var sound: Sound? = null
    /** The step this tap is on, so a failure says which one broke. */
    var failure = VoiceNoteFailure.Load
        private set
    /** The card went: a load still running then starts nothing. */
    private var released = false

    suspend fun toggle() {
        val current = sound
        if (current != null && current.playing && phase == AudioPhase.Playing) {
            current.pause()
            elapsed = current.position
            phase = AudioPhase.Paused
            return
        }
        // Taps arrive on the main thread: one while the sound loads is dropped, so it never starts a second player.
        if (phase == AudioPhase.Loading) return
        val fromStart = phase == AudioPhase.Idle || phase == AudioPhase.Failed
        phase = AudioPhase.Loading
        failure = VoiceNoteFailure.Load
        try {
            val ready = current ?: load().let { file ->
                if (released) return
                open(file) { stop ->
                    when (stop) {
                        SoundStop.Ended -> finished()
                        SoundStop.Interrupted -> if (phase == AudioPhase.Playing) interrupted()
                        SoundStop.Broke -> broke()
                    }
                }.also { sound = it; duration = it.duration }
            }
            failure = VoiceNoteFailure.Playback
            if (ready.start()) { elapsed = ready.position; phase = AudioPhase.Playing } else { drop(); phase = AudioPhase.Failed }
        } catch (cancelled: CancellationException) {
            // The tap's scope went (a dismissed preview) while this card's player lives on: the next tap starts over.
            if (phase == AudioPhase.Loading) phase = if (sound != null) AudioPhase.Paused else AudioPhase.Idle
            throw cancelled
        }
        catch (_: Exception) { drop(); phase = AudioPhase.Failed }
        if (fromStart) Analytics.capture(AnalyticsEvent.VoiceNotePlayed(failure.takeIf { phase == AudioPhase.Failed }))
    }

    /** Moves a loaded sound's playhead, held to its length; playing or paused, it stays so. */
    fun seek(seconds: Double) {
        val current = sound ?: return
        if (duration <= 0.0) return
        elapsed = seconds.coerceIn(0.0, duration)
        current.seek(elapsed)
    }

    /** Played to the end: back to a replayable start, the duration kept. */
    private fun finished() {
        elapsed = 0.0
        phase = AudioPhase.Idle
    }

    /** Lost the audio focus to another app: kept, paused. */
    private fun interrupted() {
        elapsed = sound?.position ?: 0.0
        phase = AudioPhase.Paused
    }

    /** Broke while playing: dropped, so the next tap fetches and decodes again. */
    private fun broke() {
        drop()
        phase = AudioPhase.Failed
    }

    fun release() {
        released = true
        drop()
    }

    private fun drop() {
        sound?.release()
        sound = null
        duration = 0.0
        elapsed = 0.0
    }

    /** The card's progress ticker keeps [elapsed] honest. */
    fun syncElapsed() {
        sound?.let { if (it.playing || phase == AudioPhase.Paused) elapsed = it.position }
    }
}

/** A voice note is speech played as media (the media volume, not the ringer's). */
private val voiceNoteAudio = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()

/** A voice note through `MediaPlayer`, holding the audio focus while it plays as iOS holds its audio session. */
private class MediaSound(private val manager: AudioManager, private val player: MediaPlayer,
                         private val stopped: (SoundStop) -> Unit) : Sound {
    /** Other apps' sound pauses while the note plays and picks up again after. */
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(voiceNoteAudio)
        .setOnAudioFocusChangeListener { if (it != AudioManager.AUDIOFOCUS_GAIN) { pause(); stopped(SoundStop.Interrupted) } }.build()

    init {
        player.setOnCompletionListener { manager.abandonAudioFocusRequest(focus); stopped(SoundStop.Ended) }
        player.setOnErrorListener { _, _, _ -> manager.abandonAudioFocusRequest(focus); stopped(SoundStop.Broke); true }
    }

    override val playing get() = player.isPlaying
    override val duration get() = player.duration / 1000.0
    override val position get() = runCatching { player.currentPosition / 1000.0 }.getOrDefault(0.0)

    override fun start(): Boolean {
        if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) return false
        player.start()
        return true
    }

    override fun pause() {
        if (player.isPlaying) player.pause()
        manager.abandonAudioFocusRequest(focus)
    }

    override fun seek(seconds: Double) = player.seekTo((seconds * 1000).toLong(), MediaPlayer.SEEK_CLOSEST)

    override fun release() {
        player.release()
        manager.abandonAudioFocusRequest(focus)
    }

    companion object {
        /** Decodes [file] off the main thread (`prepareAsync`), so a long recording never freezes the chat. */
        suspend fun open(manager: AudioManager, file: File, stopped: (SoundStop) -> Unit): MediaSound {
            val player = MediaPlayer()
            try {
                player.setAudioAttributes(voiceNoteAudio)
                player.setDataSource(file.path)
                suspendCancellableCoroutine<Unit> { prepared ->
                    player.setOnPreparedListener { prepared.resume(Unit) }
                    player.setOnErrorListener { _, what, extra ->
                        prepared.resumeWithException(IOException("Couldn't open the voice note ($what, $extra)")); true
                    }
                    player.prepareAsync()
                }
            } catch (error: Throwable) { player.release(); throw error }
            return MediaSound(manager, player, stopped)
        }
    }
}

/** One card's player, remembered by the file and released with it. */
@Composable internal fun rememberSharedAudio(entry: Entry, file: SharedFile, store: AppStore): SharedAudio {
    val manager = LocalContext.current.getSystemService(AudioManager::class.java)
    val audio = remember(file.id) {
        SharedAudio(load = { store.sharedFile(entry.id, file) },
            open = { path, stopped -> MediaSound.open(manager, path, stopped) })
    }
    DisposableEffect(audio) { onDispose { audio.release() } }
    return audio
}

/** A voice note's play/pause/loading, in place of its file icon: a 48 dp target over the icon's 24 dp slot, so the
 *  name lines up with the other file cards. */
@Composable internal fun SharedAudioButton(audio: SharedAudio, tapped: () -> Unit) {
    Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.requiredSize(48.dp).plainButton(action = tapped), contentAlignment = Alignment.Center) {
            when (audio.phase) {
                AudioPhase.Loading -> Spinner(24.dp, color = Palette.text)
                AudioPhase.Playing -> Symbol("pause.circle", size = 24.dp, tint = Palette.text)
                else -> Symbol("play.circle", size = 24.dp, tint = Palette.text)
            }
        }
    }
}
