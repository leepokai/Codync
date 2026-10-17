package com.codync.android

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.icu.text.Transliterator
import android.icu.util.ULocale
import android.media.*
import android.media.audiofx.AcousticEchoCanceler
import android.os.LocaleList
import androidx.core.content.ContextCompat
import com.codync.android.core.*
import java.io.IOException
import java.util.Locale
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.*

// The phone's audio for calls: the microphone and the players the voice engines in core drive.

internal val callAudio: AudioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()

/** The microphone: 16 kHz mono PCM16 frames with their level, echo-cancelled so the bot's voice isn't heard back. */
internal class AndroidMic(private val context: Context) : MicSource {
    private val worker = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var reading: Job? = null
    @Volatile private var recorder: AudioRecord? = null
    val recordingSessionId: Int? get() = recorder?.audioSessionId

    override suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit) {
        check(reading == null)
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED)
            throw IOException("Allow microphone access before starting voice chat.")
        val microphone = withContext(Dispatchers.IO) {
            val minimum = AudioRecord.getMinBufferSize(Pcm.RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val format = AudioFormat.Builder().setSampleRate(Pcm.RATE).setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build()
            val record = try {
                check(minimum > 0)
                AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION).setAudioFormat(format)
                    .setBufferSizeInBytes(maxOf(minimum, 12_800)).build()
            } catch (_: Exception) { throw IOException(UNAVAILABLE) }
            if (record.state != AudioRecord.STATE_INITIALIZED) { record.release(); throw IOException(UNAVAILABLE) }
            record
        }
        recorder = microphone
        reading = worker.launch {
            val echo = if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(microphone.audioSessionId)?.apply { enabled = true } else null
            try {
                microphone.startRecording()
                check(microphone.recordingState == AudioRecord.RECORDSTATE_RECORDING)
                val buffer = ShortArray(1_024)
                while (isActive) {
                    val count = microphone.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
                    if (!isActive) break
                    if (count < 0) throw IOException("Microphone read failed")
                    if (count == 0) { delay(10); continue }
                    frames(Pcm.bytes(buffer, count), Pcm.level(buffer, count))
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { if (isActive) failed() }
            finally {
                runCatching { microphone.stop() }; echo?.release(); microphone.release()
                if (recorder === microphone) recorder = null
            }
        }
    }

    override suspend fun stop() = withContext(NonCancellable) {
        val old = reading; reading = null
        old?.cancel(); runCatching { recorder?.stop() }; old?.join()
        Unit
    }

    private companion object { const val UNAVAILABLE = "Couldn't use the microphone. Check its permission and other audio apps." }
}

/**
 * Streams the model's 24 kHz voice. One writer thread owns the track: it writes chunks in order and
 * reports each once the playback head has passed it, so the call knows when the model stops talking.
 */
internal class AndroidPcmPlayer : PcmPlayer {
    private class Chunk(val pcm: ByteArray, val played: () -> Unit)
    private val chunks = LinkedBlockingQueue<Chunk>()
    @Volatile private var flushes = 0
    @Volatile private var closed = false
    private val track = AudioTrack.Builder().setAudioAttributes(callAudio)
        .setAudioFormat(AudioFormat.Builder().setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
        .setTransferMode(AudioTrack.MODE_STREAM)
        .setBufferSizeInBytes(maxOf(AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT), RATE * 2 / 5))
        .build().apply { play() }
    private val writer = thread(name = "codync-call-playback") { write() }

    override fun play(pcm: ByteArray, played: () -> Unit) { chunks += Chunk(pcm, played) }
    override fun stop() { chunks.clear(); flushes++ }
    override suspend fun close() = withContext(Dispatchers.IO) { closed = true; writer.join(); track.release() }

    private fun write() {
        val waiting = ArrayDeque<Pair<Long, () -> Unit>>()
        var written = 0L; var flushed = flushes; var padded = false; var stalled = 0; var last = -1L
        while (!closed) {
            if (flushed != flushes) {
                flushed = flushes; waiting.clear(); written = 0; padded = false; last = -1
                track.pause(); track.flush(); track.play()
            }
            val next = chunks.poll(20, TimeUnit.MILLISECONDS)
            if (next != null && flushed == flushes) {
                track.write(next.pcm, 0, next.pcm.size and 1.inv(), AudioTrack.WRITE_BLOCKING)
                written += next.pcm.size / 2; waiting.addLast(written to next.played); padded = false
            }
            val head = track.playbackHeadPosition.toLong() and 0xFFFF_FFFFL
            while (waiting.isNotEmpty() && head >= waiting.first().first) waiting.removeFirst().second()
            if (head != last) { last = head; stalled = 0 } else if (next == null) stalled++
            // A short last chunk can sit under the track's start threshold: push it out with silence.
            if (waiting.isNotEmpty() && !padded && stalled > 10) {
                track.write(ByteArray(RATE / 5 * 2), 0, RATE / 5 * 2, AudioTrack.WRITE_BLOCKING); padded = true
            }
        }
        track.pause(); track.flush()
    }

    private companion object { const val RATE = 24_000 }
}

/** Plays one text-to-speech clip from memory, through the call's audio route. */
internal object AndroidClipPlayer : ClipPlayer {
    override suspend fun play(audio: ByteArray, format: String) = suspendCancellableCoroutine { done ->
        val player = MediaPlayer()
        var released = false
        fun release() { if (!released) { released = true; runCatching { player.stop() }; player.release() } }
        player.setAudioAttributes(callAudio)
        player.setDataSource(object : MediaDataSource() {
            override fun readAt(position: Long, buffer: ByteArray, offset: Int, size: Int): Int {
                if (position >= audio.size) return -1
                val count = minOf(size.toLong(), audio.size - position).toInt()
                System.arraycopy(audio, position.toInt(), buffer, offset, count)
                return count
            }
            override fun getSize(): Long = audio.size.toLong()
            override fun close() { }
        })
        player.setOnPreparedListener { it.start() }
        player.setOnCompletionListener { release(); if (done.isActive) done.resume(Unit) }
        player.setOnErrorListener { _, _, _ -> release(); if (done.isActive) done.resumeWithException(IOException("Couldn't play the $format reply.")); true }
        done.invokeOnCancellation { player.setOnCompletionListener(null); release() }
        player.prepareAsync()
    }
}

/**
 * Speech to text writes Chinese in Simplified characters whatever it's told: turn it into Traditional
 * for someone who reads Traditional (their first Chinese language, or a Traditional region when none is listed).
 */
internal fun inReadersScript(text: String): String {
    val languages = LocaleList.getDefault().let { list -> (0 until list.size()).map(list::get) }
    val chinese = languages.firstOrNull { it.language == "zh" }
    val traditional = if (chinese != null) ULocale.addLikelySubtags(ULocale.forLocale(chinese)).script == "Hant"
        else Locale.getDefault().country in listOf("TW", "HK", "MO")
    return if (traditional) Transliterator.getInstance("Hans-Hant").transliterate(text) else text
}
