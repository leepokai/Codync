package com.codync.android

import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.MediaMetadataRetriever
import android.net.ConnectivityManager
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import java.io.File
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

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
    /** Where the one fetch runs: the card's, so a preload or tap that stops waiting leaves it to the others. */
    private val scope: CoroutineScope,
    /** How a preload reads the sound's length off its bytes without playing them ([headerLength]; tests stub it). */
    private val length: suspend (File) -> Double = ::headerLength,
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
    /** Where a seek made before the sound opened wants it to start (seconds), applied as it opens. */
    private var pendingSeek: Double? = null
    /** The step this tap is on, so a failure says which one broke. */
    var failure = VoiceNoteFailure.Load
        private set
    /** The card went: a load still running then starts nothing. */
    private var released = false
    /** The length the preload read off the bytes (0 until then): shown before the first play and after a failure. */
    private var known = 0.0
    /** The one fetch of the sound's bytes preload and a play tap share while it runs; once it answers it is forgotten,
     *  so the next ask reads the file from the store's cache (or fetches again when that file went). */
    private var fetching: Deferred<Result<File>>? = null

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
            val ready = current ?: bytes().let { file ->
                if (released) return
                open(file) { stop ->
                    when (stop) {
                        SoundStop.Ended -> finished()
                        SoundStop.Interrupted -> if (phase == AudioPhase.Playing) interrupted()
                        SoundStop.Broke -> broke()
                    }
                }.also {
                    if (released) { it.release(); return }
                    sound = it
                    duration = it.duration
                    pendingSeek?.let { seconds -> it.seek(seconds); pendingSeek = null }
                }
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

    /** Ready the sound before a tap asks for it: fetches its bytes (the same fetch [toggle] would share) and reads
     *  the sound's length off them, so the bar and clock show before the first play. A fetch that fails stays quiet:
     *  it is forgotten and the tap retries and reports it. */
    suspend fun preload() {
        val seconds = try {
            val file = bytes()
            // A tap already on these bytes, or a sound once played, says its length itself.
            if (released || phase != AudioPhase.Idle || duration > 0.0) return
            length(file)
        } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { return }
        if (!released && duration <= 0.0) { known = seconds; duration = seconds }
    }

    /** The sound's bytes from the computer, one fetch for everyone asking while it runs: it runs in the card's [scope],
     *  so a caller cancelled while waiting (a preload restarting) doesn't fail it for the others, and it holds a
     *  [Result] so a failed fetch doesn't cancel the card's scope. Once it answers it is forgotten, so the next ask
     *  reads the file from the store's cache, which fetches again when the attachment cache has let it go. */
    private suspend fun bytes(): File {
        val running = fetching ?: scope.async { runCatching { load() } }.also { fetching = it }
        val result = running.await()
        if (fetching === running) fetching = null
        return result.getOrThrow()
    }

    /** Moves the playhead, held to the sound's length; playing or paused, it stays so. Before the first play, the
     *  preloaded length shows the move and the sound opens there. */
    fun seek(seconds: Double) {
        if (duration <= 0.0) return
        elapsed = seconds.coerceIn(0.0, duration)
        val current = sound
        if (current != null) current.seek(elapsed) else pendingSeek = elapsed
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
        pendingSeek = null
        fetching = null
        duration = known
        elapsed = 0.0
    }

    /** The card's progress ticker keeps [elapsed] honest. */
    fun syncElapsed() {
        sound?.let { if (it.playing || phase == AudioPhase.Paused) elapsed = it.position }
    }
}

/** A sound's length off its bytes' headers, in seconds (0 when they don't say); it parses, never plays. Off the
 *  caller's thread: the retriever scans the file. */
private suspend fun headerLength(file: File): Double = withContext(Dispatchers.IO) {
    MediaMetadataRetriever().use { retriever ->
        retriever.setDataSource(file.path)
        retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toDoubleOrNull()?.let { it / 1000.0 } ?: 0.0
    }
}

/** A voice note is speech played as media (the media volume, not the ringer's). Lazy, so the file's top-level
 *  functions (`voiceNotePreloads`) load in unit tests without the platform. */
private val voiceNoteAudio by lazy {
    AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
}

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

/** One card's player, remembered by the file and released with it.
 *  [online]: the computer is connected, so the note's sound can be fetched ahead of a tap. */
@Composable internal fun rememberSharedAudio(entry: Entry, file: SharedFile, store: AppStore, online: Boolean): SharedAudio {
    val context = LocalContext.current
    val manager = context.getSystemService(AudioManager::class.java)
    val scope = rememberCoroutineScope()
    val audio = remember(file.id) {
        SharedAudio(load = { store.sharedFile(entry.id, file) },
            open = { path, stopped -> MediaSound.open(manager, path, stopped) },
            scope = scope)
    }
    DisposableEffect(audio) { onDispose { audio.release() } }
    // The sound preloads so a tap plays at once — unless this network is metered and the note is a big one, and
    // never while the computer is away (a chat shown before it connects fetches nothing).
    LaunchedEffect(audio, online) {
        val metered = context.getSystemService(ConnectivityManager::class.java)?.isActiveNetworkMetered ?: true
        if (voiceNotePreloads(online, metered, file.size)) audio.preload()
    }
    return audio
}

/** The sound's bytes big enough to skip on a network that charges for them (2 MiB; iOS `preloadLimit`). */
private const val PRELOAD_LIMIT = 2 * 1024 * 1024L

/** Whether the card preloads a sound as it shows: the computer online, and the fetch never costly — under
 *  [PRELOAD_LIMIT] even on a metered network (cellular, a hotspot; iOS `preloads`). */
internal fun voiceNotePreloads(online: Boolean, metered: Boolean, bytes: Long): Boolean =
    online && (!metered || bytes < PRELOAD_LIMIT)

/** A voice note's play/pause/loading, in place of its file icon: a 48 dp target over the icon's 24 dp slot, so the
 *  name lines up with the other file cards, with a soft round chip that fades in behind the glyph while pressed
 *  (a phone has no hover). */
@Composable internal fun SharedAudioButton(audio: SharedAudio, tapped: () -> Unit) {
    val (source, pressed) = rememberPress()
    val chip by animateFloatAsState(if (pressed) 0.14f else 0f, Motion.press(), label = "Play pressed")
    Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.requiredSize(48.dp).clickable(source, null, role = Role.Button, onClick = tapped),
            contentAlignment = Alignment.Center) {
            Box(Modifier.size(36.dp).clip(CircleShape).background(Palette.text.copy(alpha = chip)))
            when (audio.phase) {
                AudioPhase.Loading -> Spinner(24.dp, color = Palette.text)
                AudioPhase.Playing -> Symbol("pause.circle", size = 24.dp, tint = Palette.text)
                else -> Symbol("play.circle", size = 24.dp, tint = Palette.text)
            }
        }
    }
}
