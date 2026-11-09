package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/** The computer's remote screen: whether phones can view/control it, and who's in control (CodyncKit `ScreenState`). */
@Serializable
data class ScreenState(
    val enabled: Boolean = false,
    /** The screen helper is running. */
    val connected: Boolean = false,
    val platform: String = "",
    /** App name shown in the computer's system permission settings. */
    val permissionApp: String? = null,
    /** Screen recording is permitted. */
    val capture: Boolean = false,
    /** Input injection is permitted. */
    val input: Boolean = false,
    val displays: List<ScreenDisplay> = emptyList(),
    /** A phone took over: bots may only look. */
    val userControl: Boolean = false,
    /** The bot using the computer right now. */
    val agentBot: String? = null,
    /** Bots with computer use can act here (Windows: always; elsewhere Remote screen is on). */
    val computerUse: Boolean = false,
    val viewers: Int = 0,
) {
    val available: Boolean get() = enabled && connected && capture
    val controlReady: Boolean get() = if (platform == "windows") computerUse else available && input
    val mainDisplay: ScreenDisplay? get() = displays.firstOrNull { it.main } ?: displays.firstOrNull()

    /** Remote clients explain setup; only the computer itself can request OS permissions. */
    val accessGuidance: String get() {
        if (platform == "windows") return "Remote screen viewing is not available on Windows yet. Bots can still use apps on that computer."
        val setup = "On the computer, open Codync → Settings → Computer access. Permissions are set separately on each computer."
        val app = permissionApp ?: "Codync Screen"
        return when {
            !enabled -> "Remote screen is off. $setup"
            !connected -> "Codync Screen is not running. $setup"
            platform == "linux" && (!capture || !input) -> "Choose a screen and allow control in the system sharing dialog. $setup"
            !capture -> "Allow screen recording for $app. $setup"
            !input -> "Allow computer control for $app. macOS may call this Device Control and Data Access or Accessibility. $setup"
            else -> "Computer access is ready."
        }
    }

    companion object {
        fun decode(value: JsonObject): ScreenState = WireJson.decodeFromJsonElement(value)
    }
}

@Serializable
data class ScreenDisplay(
    val id: Long,
    val name: String,
    /** Size in points: the coordinate space of screen input. */
    val width: Double,
    val height: Double,
    val main: Boolean = false,
)

@Serializable data class ScreenIceServer(val urls: List<String>, val username: String? = null, val credential: String? = null)

/** A reserved viewer session and the ICE servers to reach it (`screenPrepare`). */
@Serializable
data class ScreenConnection(val session: String, val iceServers: List<ScreenIceServer>, /** Unix milliseconds. */ val expiresAt: Long,
    /** False when an older host or helper requires complete SDP. */ val trickle: Boolean = false) {
    /** Renew five minutes before the TURN credentials expire, never sooner than a second from now. */
    fun renewIn(nowMillis: Long): Long = maxOf(1_000, expiresAt - nowMillis - 300_000)
}

@Serializable data class ScreenAnswer(val session: String, val sdp: String)

/** What the computer said on the `input` channel. */
sealed interface ScreenReply {
    /** Text the computer copied. */
    data class Clipboard(val text: String?) : ScreenReply
    /** The region the video now shows (`null`: the whole display). */
    data class View(val crop: ScreenRect?) : ScreenReply
    data class Error(val message: String?) : ScreenReply

    companion object {
        fun decode(text: String): ScreenReply? {
            val msg = runCatching { WireJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: return null
            return when (msg.text("type")) {
                "clipboard" -> Clipboard(msg.text("text"))
                "view" -> View((msg["crop"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.doubleOrNull }
                    ?.takeIf { it.size == 4 }?.let { ScreenRect(it[0], it[1], it[2], it[3]) })
                "error" -> Error(msg.text("message"))
                else -> null
            }
        }
    }
}

/** The messages a viewer sends besides pointer and keys (`ScreenSession.view`, `switchDisplay`, `pushClipboard`). */
object ScreenRequests {
    /** Stream a region of the display (points; `null` = all of it) at the viewer's pixel size. */
    fun view(display: Long?, crop: ScreenRect?, pixels: ScreenSize): JsonObject = buildJsonObject {
        put("type", "view")
        put("long", maxOf(pixels.width, pixels.height))
        put("short", minOf(pixels.width, pixels.height))
        if (display != null) put("display", display)
        if (crop != null) put("crop", buildJsonArray { add(crop.x); add(crop.y); add(crop.width); add(crop.height) })
    }

    fun switchDisplay(display: Long): JsonObject = buildJsonObject { put("type", "view"); put("display", display) }

    fun clipboard(text: String): JsonObject = buildJsonObject { put("type", "clipboard"); put("text", text) }

    /** A key by name (`return`, `left`, `c`) with the modifiers held for it. */
    fun key(name: String, modifiers: List<String>): JsonObject = buildJsonObject {
        put("type", "key"); put("key", name)
        put("modifiers", buildJsonArray { modifiers.forEach { add(it) } })
    }
}

/**
 * Hardware H.264 first (High, then Constrained Baseline), every other codec only as a fallback.
 * `profile` reads a codec's `profile-level-id`.
 */
fun <T> preferredScreenCodecs(codecs: List<T>, name: (T) -> String, profile: (T) -> String?): List<T> {
    val h264 = codecs.filter { name(it) == "H264" }.sortedByDescending { profile(it).orEmpty().startsWith("640c") }
    return h264 + codecs.filter { name(it) != "H264" }
}
