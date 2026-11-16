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

/** One chat's wait from its opening action to its messages' first drawn frame. Reports only its own first frame. */
internal class OpenToMessages(private val nowNanos: () -> Long = System::nanoTime) {
    private var opening: Pair<String, Long>? = null

    fun open(botId: String?) {
        when {
            botId == null -> opening = null
            opening?.first == botId -> return
            else -> opening = botId to nowNanos()
        }
    }

    fun finish(botId: String): Long? {
        val (pending, started) = opening ?: return null
        if (pending != botId) return null
        opening = null
        return (nowNanos() - started) / 1_000_000
    }
}
