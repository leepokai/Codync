package com.codync.android.core

import kotlin.math.max
import kotlin.math.min

data class ScreenPoint(val x: Double, val y: Double)
data class ScreenSize(val width: Double, val height: Double)
data class ScreenRect(val x: Double, val y: Double, val width: Double, val height: Double) {
    val maxX: Double get() = x + width
    val maxY: Double get() = y + height

    fun intersection(other: ScreenRect): ScreenRect {
        val left = max(x, other.x)
        val top = max(y, other.y)
        return ScreenRect(left, top, max(0.0, min(maxX, other.maxX) - left), max(0.0, min(maxY, other.maxY) - top))
    }
}

/**
 * Which part of a remote display the viewer shows, and the mapping between view points and
 * display points (CodyncKit `ScreenViewport`). Zoom 1 fits the whole display; beyond that the
 * visible region takes the view's aspect and is kept on the display.
 */
data class ScreenViewport(
    /** Display size in points. */
    val display: ScreenSize,
    /** Viewer size in points. */
    val view: ScreenSize,
    val zoom: Double = 1.0,
    /** The display point at the middle of the view. */
    val center: ScreenPoint = ScreenPoint(display.width / 2, display.height / 2),
) {
    private val fitScale: Double get() {
        // Before layout the view is 0×0: any positive scale keeps the math finite.
        if (display.width <= 0 || display.height <= 0 || view.width <= 0 || view.height <= 0) return 1.0
        return min(view.width / display.width, view.height / display.height)
    }

    val maxZoom: Double get() = max(1.0, MAX_POINT_SCALE / max(fitScale, 0.0001))

    /** View points per display point. */
    val scale: Double get() = fitScale * zoom

    /** The display region the view covers (may extend past the display when letterboxed). */
    val visible: ScreenRect get() {
        val w = view.width / scale
        val h = view.height / scale
        return ScreenRect(center.x - w / 2, center.y - h / 2, w, h)
    }

    val displayBounds: ScreenRect get() = ScreenRect(0.0, 0.0, display.width, display.height)

    /** The region to stream: `null` while the whole display is in view. */
    val crop: ScreenRect? get() = if (zoom <= 1.001) null else visible.intersection(displayBounds)

    fun toDisplay(p: ScreenPoint): ScreenPoint {
        val v = visible
        return ScreenPoint(v.x + p.x / scale, v.y + p.y / scale)
    }

    fun toView(p: ScreenPoint): ScreenPoint {
        val v = visible
        return ScreenPoint((p.x - v.x) * scale, (p.y - v.y) * scale)
    }

    /** Where a streamed region (display points) sits in the view. */
    fun frame(region: ScreenRect): ScreenRect {
        val o = toView(ScreenPoint(region.x, region.y))
        return ScreenRect(o.x, o.y, region.width * scale, region.height * scale)
    }

    /** Pinch: zoom by `factor` keeping the display point under `anchor` (view point) in place. */
    fun zoomed(factor: Double, anchor: ScreenPoint): ScreenViewport {
        val fixed = toDisplay(anchor)
        val next = copy(zoom = min(max(zoom * factor, 1.0), maxZoom))
        val s = next.scale
        return next.copy(center = ScreenPoint(fixed.x - (anchor.x - view.width / 2) / s, fixed.y - (anchor.y - view.height / 2) / s)).clamped()
    }

    /** Moves the view by a drag of `delta` view points. */
    fun panned(delta: ScreenPoint): ScreenViewport =
        copy(center = ScreenPoint(center.x - delta.x / scale, center.y - delta.y / scale)).clamped()

    /** Scrolls just enough to keep `p` (display point) inside the inner 80% of the view. */
    fun following(p: ScreenPoint): ScreenViewport {
        val v = visible
        val mx = v.width * 0.1
        val my = v.height * 0.1
        val cx = when {
            p.x < v.x + mx -> center.x - (v.x + mx - p.x)
            p.x > v.maxX - mx -> center.x + (p.x - (v.maxX - mx))
            else -> center.x
        }
        val cy = when {
            p.y < v.y + my -> center.y - (v.y + my - p.y)
            p.y > v.maxY - my -> center.y + (p.y - (v.maxY - my))
            else -> center.y
        }
        return copy(center = ScreenPoint(cx, cy)).clamped()
    }

    fun reset(): ScreenViewport = copy(zoom = 1.0, center = ScreenPoint(display.width / 2, display.height / 2))

    /** The same region on a resized view (rotation, the keyboard), kept on the display. */
    fun resized(view: ScreenSize): ScreenViewport = copy(view = view).clamped()

    private fun clamped(): ScreenViewport {
        val v = visible
        val x = if (v.width >= display.width) display.width / 2 else min(max(center.x, v.width / 2), display.width - v.width / 2)
        val y = if (v.height >= display.height) display.height / 2 else min(max(center.y, v.height / 2), display.height - v.height / 2)
        return copy(center = ScreenPoint(x, y))
    }

    private companion object {
        /** Closest zoom: one display point spans this many view points. */
        const val MAX_POINT_SCALE = 4.0
    }
}
