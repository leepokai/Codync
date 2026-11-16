package com.codync.android

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Measurements sent to this phone's connected host for its local performance log. No row, bot or message identity. */
internal sealed interface PhonePerformance {
    fun body(): JsonObject

    data class ChatOpen(val durationMs: Long) : PhonePerformance {
        override fun body() = buildJsonObject {
            put("type", "chat_open")
            put("platform", "android")
            put("durationMs", durationMs)
        }
    }

    data class Scroll(val list: ScrollList, val rows: Int, val frames: Int, val hitches: Int, val longestMs: Int,
        val hitchMsPerS: Int, val fps: Int, val stop: ScrollStop? = null, val jump: Int? = null) : PhonePerformance {
        override fun body() = buildJsonObject {
            put("type", "scroll")
            put("platform", "android")
            put("list", list.wire)
            put("rows", rows)
            put("frames", frames)
            put("hitches", hitches)
            put("longestMs", longestMs)
            put("hitchMsPerS", hitchMsPerS)
            put("fps", fps)
            stop?.let { put("stop", it.wire) }
            jump?.let { put("jump", it) }
        }
    }
}

internal enum class ScrollList(val wire: String) { Bots("bots"), Chat("chat") }
internal enum class ScrollStop(val wire: String) { Top("top"), End("end"), Middle("middle") }

/** One opening's wait for its messages to reach their first drawn frame. A second finish reports nothing. */
internal class OpenToMessages(private val nowNanos: () -> Long = System::nanoTime) {
    private var openedAt: Long? = null

    fun begin() {
        openedAt = nowNanos()
    }

    fun finish(): Long? {
        val started = openedAt ?: return null
        openedAt = null
        return (nowNanos() - started) / 1_000_000
    }
}
