package com.codync.android

import android.content.Context
import com.codync.android.core.MicSource
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlinx.coroutines.*

/** A recorded clip's 16 kHz mono PCM16 played in real time, as the microphone would; nothing is played aloud. */
internal class ClipMic(private val pcm: ByteArray) : MicSource {
    private var playing: Job? = null
    override suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit) {
        playing = CoroutineScope(Dispatchers.IO).launch {
            for (start in pcm.indices step CHUNK) {
                frames(pcm.copyOfRange(start, minOf(start + CHUNK, pcm.size)), 0.5f)
                delay(100)
            }
        }
    }
    override suspend fun stop() { playing?.cancelAndJoin() }

    companion object {
        /** 100 ms of audio per frame. */
        private const val CHUNK = 3_200

        /**
         * Debug builds only: `dictation-clip.wav` in the app's files folder replaces the microphone for dictation, for
         * end-to-end checks on an emulator without a usable microphone.
         */
        fun debugClip(context: Context): ClipMic? {
            if (!BuildConfig.DEBUG) return null
            val file = File(context.filesDir, "dictation-clip.wav").takeIf { it.isFile } ?: return null
            return ClipMic(samples(file.readBytes()))
        }

        /** A WAV's `data` chunk. */
        fun samples(wav: ByteArray): ByteArray {
            val header = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
            var offset = 12
            while (offset + 8 <= wav.size) {
                val size = header.getInt(offset + 4)
                if (String(wav, offset, 4, Charsets.US_ASCII) == "data") return wav.copyOfRange(offset + 8, minOf(offset + 8 + size, wav.size))
                offset += 8 + size + size % 2
            }
            error("The clip has no audio")
        }
    }
}
