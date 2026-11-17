package com.codync.android.design

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sign
import kotlin.math.sin
import kotlin.math.sqrt

// The thinking orb: a halftone disc on the avatars' kind of dot grid. Dots never move; each state is
// a field that sets how full every dot is, so the light flows across the grid. Line for line with
// CodyncKit `ThinkingOrbGeometry.swift` and the desktop's lib/thinking-orb.ts; all three are checked
// against apps/ios/Kit/Tests/CodyncKitTests/Fixtures/thinking-orb-frames.json.
internal object ThinkingOrbGeometry {
    data class Dot(val x: Double, val y: Double, val radius: Double, val alpha: Double)

    /** One step coarser than the avatars' grid: a status glyph sits beside text. */
    fun cells(size: Double) = if (size < 18) 5 else if (size < 30) 7 else 9

    /** The pose a still orb shows (Remove animations) and a moving one starts from, in seconds into its animation. */
    fun rest(state: ThinkingOrbState) = when (state) {
        ThinkingOrbState.Working, ThinkingOrbState.Searching -> 0.45
        ThinkingOrbState.Listening -> 0.55
        ThinkingOrbState.Connecting -> 0.5
        ThinkingOrbState.Shaping -> 1.9
    }

    /** Every dot of the orb in a `size` box at `time` seconds: the grid cropped to a circle. */
    fun frame(state: ThinkingOrbState, size: Double, time: Double): List<Dot> {
        if (!size.isFinite() || size <= 0 || !time.isFinite()) return emptyList()
        val n = cells(size)
        val half = n / 2.0
        val pitch = size / n
        val dots = mutableListOf<Dot>()
        for (row in 0 until n) for (col in 0 until n) {
            val u = (col + 0.5 - half) / half
            val w = (row + 0.5 - half) / half
            if (hypot(u, w) > 1.0001) continue
            val v = clamp01(fill(state, u, w, time))
            // A full dot nearly touches its neighbors; an empty one stays a faint grain so the disc keeps its outline.
            dots += Dot((col + 0.5) * pitch, (row + 0.5) * pitch, pitch * (0.14 + 0.36 * v), 0.5 + 0.5 * v)
        }
        return dots
    }

    /** How full the dot at (`u`, `w`) on the unit disc is (0…1), `t` seconds in. */
    private fun fill(state: ThinkingOrbState, u: Double, w: Double, t: Double): Double = when (state) {
        ThinkingOrbState.Working -> {
            // A halftone-shaded ball whose light circles it clockwise every 2 s.
            val a = t * 2 * PI / 2 - 3 * PI / 4
            val nz = sqrt(max(0.0, 1 - u * u - w * w))
            smooth((0.85 * (u * cos(a) + w * sin(a)) + 0.53 * nz - 0.3) / 0.62)
        }
        ThinkingOrbState.Searching -> {
            // A vertical beam sweeping left and right.
            val x = (pingPong(t / 2.4) * 2 - 1) * 0.85
            gauss(u - x, 0.25)
        }
        ThinkingOrbState.Listening -> {
            // Rings opening from the center, two at a time.
            val rho = hypot(u, w)
            (0..1).maxOf { k ->
                val phase = fract(t / 1.9 + k * 0.5)
                gauss(rho - phase * 1.1, 0.22) * (1 - phase).pow(0.45)
            }
        }
        ThinkingOrbState.Connecting -> {
            // Two corners lit, a pulse running between them.
            val m = pingPong(t / 1.8)
            val ax = -0.48; val ay = 0.48; val bx = 0.48; val by = -0.48
            val px = lerp(ax, bx, m)
            val py = lerp(ay, by, m)
            maxOf(0.75 * gauss(hypot(u - ax, w - ay), 0.24), 0.75 * gauss(hypot(u - bx, w - by), 0.24), gauss(hypot(u - px, w - py), 0.26))
        }
        ThinkingOrbState.Shaping -> {
            // A filled circle, triangle and square in turn, melting into each other.
            val hold = 1.1
            val change = 0.6
            val segment = hold + change
            val cycle = fract(t / (segment * 3)) * segment * 3
            val k = min(2, (cycle / segment).toInt())
            val local = cycle - k * segment
            val m = if (local > hold) smooth((local - hold) / change) else 0.0
            val d = lerp(outline(k, u, w), outline((k + 1) % 3, u, w), m)
            1 - smooth((d + 0.1) / 0.22)
        }
    }

    /** Signed distance (negative inside, y down) to a circle, a triangle pointing up, or a square. */
    private fun outline(shape: Int, u: Double, w: Double): Double = when (shape) {
        0 -> hypot(u, w) - 0.62
        1 -> {
            val r = 0.62
            val k = sqrt(3.0)
            var x = abs(u) - r
            var y = -(w - 0.04) + r / k
            if (x + k * y > 0) { val nx = (x - k * y) / 2; y = (-k * x - y) / 2; x = nx }
            x -= max(-2 * r, min(0.0, x))
            -hypot(x, y) * sign(y)
        }
        else -> {
            val dx = abs(u) - 0.5
            val dy = abs(w) - 0.5
            hypot(max(dx, 0.0), max(dy, 0.0)) + min(max(dx, dy), 0.0)
        }
    }

    private fun clamp01(v: Double) = v.coerceIn(0.0, 1.0)
    private fun smooth(x: Double): Double { val c = clamp01(x); return c * c * (3 - 2 * c) }
    private fun lerp(a: Double, b: Double, m: Double) = a + (b - a) * m
    private fun gauss(d: Double, s: Double) = exp(-(d * d) / (2 * s * s))
    private fun fract(x: Double) = x - floor(x)
    /** Eased 0 → 1 → 0 over one unit of `x`. */
    private fun pingPong(x: Double): Double { val f = fract(x); return if (f < 0.5) smooth(f * 2) else smooth(2 - f * 2) }
}
