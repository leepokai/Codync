package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Private, bounded ICE signaling. Candidate contents must never be logged. */
@Serializable
data class ScreenCandidate(
    val type: Kind,
    val candidate: String? = null,
    val sdpMLineIndex: Int? = null,
    val sdpMid: String? = null,
    val message: String? = null,
) {
    @Serializable enum class Kind {
        @SerialName("ready") Ready,
        @SerialName("candidate") Candidate,
        @SerialName("complete") Complete,
        @SerialName("error") Error,
    }

    fun validate() {
        if (type != Kind.Candidate) return
        val sdp = candidate
        if (sdp == null || !sdp.startsWith("candidate:") || sdp.toByteArray(Charsets.UTF_8).size > 4096 ||
            '\r' in sdp || '\n' in sdp || sdpMLineIndex == null || sdpMLineIndex !in 0..16 ||
            (sdpMid?.toByteArray(Charsets.UTF_8)?.size ?: 0) > 256) {
            throw ScreenProtocolException("Invalid screen ICE candidate.")
        }
    }

    companion object {
        const val maxCount = 128
        val complete = ScreenCandidate(Kind.Complete)
    }
}

class ScreenProtocolException(message: String, cause: Throwable? = null) : IOException(message, cause)

fun screenCandidates(channel: EncryptedChannel, session: String): Flow<ScreenCandidate> =
    channel.subscribe("screenCandidates", buildJsonObject { put("session", session) }).map { value ->
        try {
            val event = WireJson.decodeFromJsonElement<ScreenCandidate>(value)
            event.validate()
            event
        } catch (error: IllegalArgumentException) {
            throw ScreenProtocolException("Invalid screen signaling event.", error)
        }
    }
