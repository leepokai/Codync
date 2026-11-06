package com.codync.android.core

/** Dictation's arithmetic (iOS `Dictation`'s statics): where its words go in the box, and a recording's pieces. */
object DictationText {
    private const val BYTES_PER_SECOND = Pcm.RATE * 2
    /** A transcription request has to fit one channel message once base64-encoded: 20 s of audio. */
    const val PIECE_BYTES = BYTES_PER_SECOND * 20
    /** Under half a second is a tap of the mic, not speech (and speech-to-text models invent words for silence). */
    const val MIN_BYTES = BYTES_PER_SECOND / 2
    /** Transcription requests in flight at once, under the channel's and the providers' limits. */
    const val IN_FLIGHT = 3

    /** [said] after what's already in the box, with one space between. */
    fun join(before: String, said: String): String = when {
        before.isBlank() -> said
        said.isEmpty() -> before
        before.last().isWhitespace() -> before + said
        else -> "$before $said"
    }

    /** Byte ranges (start until end) of a recording's 20 s pieces; a tail under half a second stays with the piece before. */
    fun pieces(count: Int): List<IntRange> {
        val starts = (0 until count step PIECE_BYTES).toMutableList()
        if (starts.size > 1 && count - starts.last() < MIN_BYTES) starts.removeAt(starts.lastIndex)
        return starts.mapIndexed { i, start -> start until (starts.getOrNull(i + 1) ?: count) }
    }

    /** Seconds of audio in [bytes] of PCM, for this month's minutes. */
    fun seconds(bytes: Int): Long = (bytes / BYTES_PER_SECOND).toLong()
}
