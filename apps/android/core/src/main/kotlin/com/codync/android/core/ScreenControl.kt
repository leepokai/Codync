package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

enum class TouchMode {
    /** The phone is a trackpad: drag moves the pointer, tap clicks where it is. */
    Trackpad,
    /** Tap clicks where you touch. */
    Direct,
}

/**
 * Every touch and key gesture on the remote screen turned into input for the computer, in
 * display points (CodyncUI `ScreenCanvas` without its view). Gestures arrive in view points.
 */
class ScreenControl(private val send: (JsonObject) -> Unit) {
    var mode = TouchMode.Trackpad
    var interactive = true
    /** One-shot modifiers from the on-screen bar, applied to the next key or click. */
    var armed: List<String> = emptyList()
    var onArmedConsumed: () -> Unit = {}
    /** The video moved; `request` also asks for the visible region once the gesture settles. */
    var onChanged: (request: Boolean) -> Unit = {}

    var viewport = ScreenViewport(ScreenSize(1.0, 1.0), ScreenSize(1.0, 1.0))
        private set
    /** Mid-pinch the region request waits for the fingers to lift. */
    var pinching = false
        private set
    private var streamRegion: ScreenRect? = null
    private var cursor = ScreenPoint(0.0, 0.0)
    private var lastTap: Tap? = null
    private var dragging = false
    private var lastPress = ScreenPoint(0.0, 0.0)

    private class Tap(val at: Long, val point: ScreenPoint, val count: Int)

    val zoomed: Boolean get() = viewport.crop != null
    /** Where the streamed region sits in the view. */
    val videoFrame: ScreenRect get() = viewport.frame(streamRegion ?: viewport.displayBounds)

    // MARK: state from the view

    fun setDisplay(d: ScreenDisplay) {
        val size = ScreenSize(d.width, d.height)
        if (size == viewport.display) return
        viewport = ScreenViewport(size, viewport.view)
        cursor = ScreenPoint(d.width / 2, d.height / 2)
        streamRegion = null
        onChanged(true)
    }

    fun setViewSize(size: ScreenSize) {
        if (size == viewport.view) return
        viewport = viewport.resized(size)
        onChanged(true)
    }

    /** The region the computer confirmed it streams (`null`: the whole display). */
    fun setStreamRegion(crop: ScreenRect?) {
        if (crop == streamRegion) return
        streamRegion = crop
        onChanged(false)
    }

    fun resetZoom() {
        viewport = viewport.reset()
        onChanged(true)
    }

    // MARK: gestures

    /** One finger moved by `delta` at `speed` view points a second. */
    fun onePan(delta: ScreenPoint, speed: Double) {
        if (!interactive) return pan(delta)
        when (mode) {
            TouchMode.Trackpad -> {
                // Pointer speed grows with finger speed, like a real trackpad.
                val accel = min(2.5, max(1.0, speed / 900))
                moveCursor(ScreenPoint(delta.x * accel / viewport.scale, delta.y * accel / viewport.scale))
            }
            TouchMode.Direct -> if (zoomed) pan(delta)
                else send(scroll(cursor, -delta.x, -delta.y))
        }
    }

    /** Successive quick taps become double/triple clicks without waiting to tell them apart. */
    fun tap(at: ScreenPoint, nowMillis: Long) {
        if (!interactive) return
        val p = point(at)
        cursor = p
        val last = lastTap
        val count = if (last != null && nowMillis - last.at < 350 && hypot(last.point.x - p.x, last.point.y - p.y) < 8 / viewport.scale)
            min(last.count + 1, 3) else 1
        lastTap = Tap(nowMillis, p, count)
        send(click(p, "left", count))
    }

    fun twoFingerTap(at: ScreenPoint) {
        if (!interactive) return
        send(click(point(at), "right", 1))
    }

    /** Long-press then move: drag (select text, move windows). Returns whether a drag began (for the haptic). */
    fun pressBegan(at: ScreenPoint): Boolean {
        if (!interactive) return false
        val p = point(at)
        cursor = p
        dragging = true
        send(button("down", p))
        lastPress = at
        return true
    }

    fun pressMoved(at: ScreenPoint) {
        if (!dragging) return
        if (mode == TouchMode.Direct) {
            cursor = clampToDisplay(viewport.toDisplay(at))
            send(move(cursor))
        } else {
            moveCursor(ScreenPoint((at.x - lastPress.x) / viewport.scale, (at.y - lastPress.y) / viewport.scale))
        }
        lastPress = at
    }

    /** A drag that began while interactive still lifts the button, or it stays held on the computer. */
    fun pressEnded() {
        if (!dragging) return
        send(button("up", cursor))
        dragging = false
    }

    /** Two fingers moved: scroll the computer, or move the view while zooming or only looking. */
    fun twoPan(delta: ScreenPoint, at: ScreenPoint) {
        if (pinching || !interactive) return pan(delta)
        // Natural scrolling: content follows the fingers.
        val p = if (mode == TouchMode.Direct) clampToDisplay(viewport.toDisplay(at)) else cursor
        send(scroll(p, -delta.x * 1.5, -delta.y * 1.5))
    }

    fun pinchBegan() { pinching = true }

    fun pinch(factor: Double, at: ScreenPoint) {
        viewport = viewport.zoomed(factor, at)
        onChanged(false)
    }

    fun pinchEnded() {
        pinching = false
        onChanged(true)
    }

    // MARK: keys

    /** Committed text: one character with armed modifiers is a shortcut. */
    fun typed(text: String) {
        if (!interactive) return
        val mods = consumeArmed()
        send(if (mods.isNotEmpty() && text.codePointCount(0, text.length) == 1) ScreenRequests.key(text.lowercase(), mods)
            else buildJsonObject { put("type", "text"); put("text", text) })
    }

    fun pressKey(name: String, modifiers: List<String> = emptyList()) {
        if (!interactive) return
        send(ScreenRequests.key(name, (modifiers + consumeArmed()).distinct()))
    }

    private fun consumeArmed(): List<String> {
        val mods = armed
        if (mods.isNotEmpty()) {
            armed = emptyList()
            onArmedConsumed()
        }
        return mods
    }

    // MARK: helpers

    private fun pan(delta: ScreenPoint) {
        viewport = viewport.panned(delta)
        onChanged(true)
    }

    private fun moveCursor(d: ScreenPoint) {
        val bounds = viewport.displayBounds
        cursor = ScreenPoint(min(max(cursor.x + d.x, 0.0), bounds.width - 1), min(max(cursor.y + d.y, 0.0), bounds.height - 1))
        send(move(cursor))
        val followed = viewport.following(cursor)
        if (followed != viewport) {
            viewport = followed
            onChanged(true)
        }
    }

    private fun point(at: ScreenPoint) = if (mode == TouchMode.Direct) clampToDisplay(viewport.toDisplay(at)) else cursor

    private fun clampToDisplay(p: ScreenPoint) =
        ScreenPoint(min(max(p.x, 0.0), viewport.display.width - 1), min(max(p.y, 0.0), viewport.display.height - 1))

    private fun move(p: ScreenPoint) = buildJsonObject { put("type", "move"); put("x", p.x); put("y", p.y) }

    private fun button(type: String, p: ScreenPoint) = buildJsonObject {
        put("type", type); put("x", p.x); put("y", p.y); put("button", "left")
    }

    private fun click(p: ScreenPoint, button: String, count: Int) = buildJsonObject {
        put("type", "click"); put("x", p.x); put("y", p.y); put("button", button); put("count", count)
        putJsonArray("modifiers") { consumeArmed().forEach { add(it) } }
    }

    private fun scroll(p: ScreenPoint, dx: Double, dy: Double) = buildJsonObject {
        put("type", "scroll"); put("x", p.x); put("y", p.y); put("dx", dx); put("dy", dy); put("units", "pixel")
    }
}
