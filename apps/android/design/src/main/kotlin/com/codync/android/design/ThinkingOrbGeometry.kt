package com.codync.android.design

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.acos
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

// Native port of thinking-orbs 0.3.1, commit de85557 (MIT, Jakub Antalik), line for line with
// CodyncKit `ThinkingOrbGeometry.swift`. See assets/licenses/thinking-orbs-LICENSE.txt.
internal object ThinkingOrbGeometry {
    data class Dot(val x: Double, val y: Double, val z: Double, val radius: Double, val white: Double, val alpha: Double = 1.0)
    data class Line(val x1: Double, val y1: Double, val x2: Double, val y2: Double, val white: Double, val alpha: Double, val width: Double)
    class Frame(val dots: MutableList<Dot> = mutableListOf(), val lines: MutableList<Line> = mutableListOf())

    private data class P(val x: Double, val y: Double, val z: Double) {
        operator fun plus(o: P) = P(x + o.x, y + o.y, z + o.z)
        operator fun minus(o: P) = P(x - o.x, y - o.y, z - o.z)
        operator fun times(s: Double) = P(x * s, y * s, z * s)
        operator fun div(s: Double) = P(x / s, y / s, z / s)
        val length get() = sqrt(x * x + y * y + z * z)
    }

    private data class P2(val x: Double, val y: Double) {
        operator fun plus(o: P2) = P2(x + o.x, y + o.y)
        operator fun minus(o: P2) = P2(x - o.x, y - o.y)
        operator fun times(s: Double) = P2(x * s, y * s)
    }

    fun speed(state: ThinkingOrbState, size: Double): Double {
        val small = size < 40
        return when (state) {
            ThinkingOrbState.Working -> if (small) 3.9 else 1.885
            ThinkingOrbState.Searching -> if (small) 2.665 else 2.015
            ThinkingOrbState.Listening -> if (small) 3.998 else 4.388
            ThinkingOrbState.Connecting -> if (small) 6.63 else 3.315
            ThinkingOrbState.Shaping -> if (small) 2.08 else 2.405
        }
    }

    /** `time` is the engine time, already multiplied by the preset speed. */
    fun frame(state: ThinkingOrbState, size: Double, time: Double): Frame {
        if (!size.isFinite() || size <= 0 || !time.isFinite()) return Frame()
        val frame = when (state) {
            ThinkingOrbState.Working -> orbits(size, time)
            ThinkingOrbState.Searching -> lattice(size, time, listening = false)
            ThinkingOrbState.Listening -> lattice(size, time, listening = true)
            ThinkingOrbState.Connecting -> web(size, time)
            ThinkingOrbState.Shaping -> morph(size, time)
        }
        val dots = frame.dots.filter { it.alpha >= 0.02 }.map { it.copy(radius = max(0.3, it.radius)) }.sortedBy { it.z }
        return Frame(dots.toMutableList(), frame.lines.filter { it.alpha >= 0.02 }.toMutableList())
    }

    private fun projector(yaw: Double, tilt: Double, size: Double, scale: Double): (P) -> P {
        val sy = sin(yaw); val cy = cos(yaw); val st = sin(tilt); val ct = cos(tilt)
        return { p ->
            val x = p.x * cy + p.z * sy; val z = -p.x * sy + p.z * cy
            P(size / 2 + x * scale, size / 2 - (p.y * ct - z * st) * scale, p.y * st + z * ct)
        }
    }

    private fun hash(a: Double, b: Double): Double {
        val h = sin(a * 12.9898 + b * 78.233) * 43758.5453
        return h - floor(h)
    }

    private fun noise(x: Double, y: Double): Double {
        val xi = floor(x); val yi = floor(y)
        val dx = x - xi; val dy = y - yi
        val fx = dx * dx * (3 - 2 * dx); val fy = dy * dy * (3 - 2 * dy)
        val a = hash(xi, yi); val b = hash(xi + 1, yi); val c = hash(xi, yi + 1); val d = hash(xi + 1, yi + 1)
        return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy
    }

    private fun orbits(size: Double, t: Double): Frame {
        val small = size < 40; val radius = size / 2 * 0.82
        val project = projector(t * 0.12, 0.3, size, 1.0)
        val rs = (size / 300).pow(0.6); val multiplier = if (small) 2.4 else 1.0
        val orbitCount = if (small) 3 else 12; val ghostCount = if (small) 10 else 40
        val frame = Frame()
        for (orb in 0 until orbitCount) {
            val h1 = hash(orb.toDouble(), 1.7); val h2 = hash(orb.toDouble(), 5.2); val h3 = hash(orb.toDouble(), 8.9)
            val ro = radius * (0.45 + 0.52 * h1); val theta = h1 * 2 * PI; val phi = acos(2 * h2 - 1)
            val normal = P(sin(phi) * cos(theta), cos(phi), sin(phi) * sin(theta))
            var u = P(-normal.y, normal.x, 0.0)
            u /= max(1e-6, u.length)
            val v = P(-normal.z * u.y, normal.z * u.x, normal.x * u.y - normal.y * u.x)
            val speed = (0.25 + 0.55 * h3) * (if (h3 > 0.5) 1.0 else -1.0)
            for (k in 0 until ghostCount) {
                val angle = k.toDouble() / ghostCount * 2 * PI
                val p = project((u * cos(angle) + v * sin(angle)) * ro)
                val depth = (p.z / ro + 1) / 2
                frame.dots += Dot(p.x, p.y, p.z, 0.9 * multiplier * rs, 0.72, 0.5 * (0.4 + 0.6 * depth))
            }
            for (m in 0 until 3) {
                val angle = t * speed + m / 3.0 * 2 * PI + h2 * 6
                val p = project((u * cos(angle) + v * sin(angle)) * ro)
                val depth = (p.z / ro + 1) / 2
                frame.dots += Dot(p.x, p.y, p.z, (1.2 + 1.6 * depth) * multiplier * rs, 0.3 - 0.22 * depth)
            }
        }
        return frame
    }

    private fun lattice(size: Double, t: Double, listening: Boolean): Frame {
        val small = size < 40; val rs = (size / 300).pow(0.6)
        val rings = if (listening) (if (small) 5 else 9) else (if (small) 6 else 11)
        val density = if (listening) (if (small) 13 else 23) else (if (small) 14 else 29)
        val multiplier = if (listening) (if (small) 1.6 else 1.0) else (if (small) 1.75 else 1.15)
        val radius = size / 2 * (if (listening) 0.874 else 0.82)
        val tilt = if (listening) 0.38 else 0.4 + 0.06 * sin(t * 0.35)
        val project = projector(t * (if (listening) 0.18 else 0.5), tilt, size, if (listening) 1.0 else radius)
        val scan = t * (0.5 + 1.2 * (if (small) 4.335 else 4.08))
        val frame = Frame()
        for (ri in 0..rings) {
            val lat = -PI / 2 + ri.toDouble() / rings * PI
            val cosLat = cos(lat); val sinLat = sin(lat)
            val wave = 0.62 * sin(t * 2.1 - ri * 0.52) + 0.38 * sin(t * 1.27 + ri * 0.83)
            val rr = if (listening) radius * (0.88 + 0.105 * wave) else 1.0
            val lonCount = max(1, (abs(cosLat) * density).roundToInt())
            for (j in 0 until lonCount) {
                val lon = j.toDouble() / lonCount * 2 * PI
                val p = project(P(cosLat * cos(lon), sinLat, cosLat * sin(lon)) * rr)
                val depth = (p.z / (if (listening) radius else 1.0) + 1) / 2
                if (listening) {
                    val crest = max(0.0, wave)
                    frame.dots += Dot(p.x, p.y, p.z, (0.6 + 1.7 * depth) * multiplier * (1 + 0.4 * crest) * rs,
                        0.66 - 0.56 * depth - 0.1 * crest)
                } else {
                    val angle = lon + t * 0.5 - scan
                    val delta = atan2(sin(angle), cos(angle))
                    val boost = exp(-(delta * delta) / 0.18) * max(0.0, p.z)
                    frame.dots += Dot(p.x, p.y, p.z, ((0.6 + 1.7 * depth) * multiplier + boost) * rs,
                        0.62 - 0.54 * depth, 0.45 + 0.55 * min(1.0, boost))
                }
            }
        }
        return frame
    }

    private fun web(size: Double, t: Double): Frame {
        val small = size < 40; val rs = (size / 300).pow(0.6)
        val count = if (small) 8 else 41; val signals = if (small) 1 else 7
        val multiplier = if (small) 1.52 else 0.95; val threshold = 0.72
        val project = projector(t * 0.12, 0.32, size, size / 2 * 0.8)
        val golden = PI * (3 - sqrt(5.0))
        val nodes = (0 until count).map { index ->
            val i = index.toDouble(); val y = 1 - 2 * (i + 0.5) / count
            val r = sqrt(1 - y * y); val a = i * golden
            val p = P(r * cos(a) + 0.3 * (noise(i * 0.31 + 9, t * 0.24) - 0.5) * 2,
                y + 0.3 * (noise(i * 0.53 + 27, t * 0.21) - 0.5) * 2,
                r * sin(a) + 0.3 * (noise(i * 0.77 + 55, t * 0.27) - 0.5) * 2)
            p / p.length
        }
        val frame = Frame()
        for (i in 0 until count) {
            for (j in i + 1 until count) {
                val distance = (nodes[i] - nodes[j]).length
                if (distance >= threshold) continue
                val a = project(nodes[i]); val b = project(nodes[j]); val depth = ((a.z + b.z) / 2 + 1) / 2
                frame.lines += Line(a.x, a.y, b.x, b.y, 0.42, (1 - distance / threshold) * (0.3 + 0.55 * depth), max(0.6, 0.8 * rs))
            }
            val p = project(nodes[i]); val depth = (p.z + 1) / 2
            val pulse = 1 + 0.25 * sin(t * 1.4 + i * 2.7)
            frame.dots += Dot(p.x, p.y, p.z, (1.4 + 1.8 * depth) * multiplier * pulse * rs, 0.55 - 0.45 * depth)
        }
        for (s in 0 until signals) {
            val segment = floor(t * 0.55 + s * 7.31)
            val a = floor(hash(segment, s * 3.1 + 1.7) * count).toInt()
            val b = floor(hash(segment, s * 5.7 + 4.2) * count).toInt()
            if (a == b) continue
            val tick = t * 0.55 + s * 7.31; val fraction = tick - floor(tick)
            val node = nodes[a] + (nodes[b] - nodes[a]) * fraction
            val p = project(node / max(1e-6, node.length)); val depth = (p.z + 1) / 2
            frame.dots += Dot(p.x, p.y, p.z, (1.4 * 1.5 + 1.8 * depth) * multiplier * rs, 0.05, 0.5 + 0.5 * depth)
        }
        return frame
    }

    /** A dotted outline that holds a circle, triangle and square in turn, morphing between them. */
    private fun morph(size: Double, t: Double): Frame {
        val small = size < 40; val spread = 1.45; val hold = 1.4; val change = 0.9; val segment = hold + change
        val cycle = t % (segment * shapes.size)
        val k = min(shapes.size - 1, (cycle / segment).toInt()); val local = cycle - k * segment
        val x = (local - hold) / change; val m = if (local > hold) x * x * (3 - 2 * x) else 0.0
        val from = shapes[k]; val to = shapes[(k + 1) % shapes.size]
        val samples = 160
        val points = (0 until samples).map { i ->
            val f = i.toDouble() / samples; val a = from(f); val b = to(f)
            (a + (b - a) * m) * spread
        }
        val lengths = (0 until samples).map { i -> hypot(points[(i + 1) % samples].x - points[i].x, points[(i + 1) % samples].y - points[i].y) }
        val total = lengths.sum()
        val count = if (small) 18 else 24
        val radius = max(0.35, (if (small) 0.021231 else 0.008295) * 1.35 * spread * size)
        val pulse = 1 + 0.02 * sin(local * 3.1)
        val frame = Frame()
        var i = 0; var walked = 0.0
        for (n in 0 until count) {
            val target = n.toDouble() / count * total
            while (walked + lengths[i] < target && i < samples - 1) { walked += lengths[i]; i += 1 }
            val a = points[i]; val b = points[(i + 1) % samples]
            val f = if (lengths[i] > 0) min(1.0, (target - walked) / lengths[i]) else 0.0
            val p = (a + (b - a) * f) * pulse
            frame.dots += Dot(size / 2 + p.x * size, size / 2 + p.y * size, 0.0, radius, 0.1)
        }
        return frame
    }

    private val shapes: List<(Double) -> P2> = listOf(
        { f -> P2(cos(-PI / 2 + f * 2 * PI), sin(-PI / 2 + f * 2 * PI)) * 0.24 },
        polygon(listOf(P2(0.0, -0.26), P2(0.24, 0.16), P2(-0.24, 0.16))),
        polygon(listOf(P2(0.0, -0.2), P2(0.2, -0.2), P2(0.2, 0.2), P2(-0.2, 0.2), P2(-0.2, -0.2))),
    )

    /** The closed outline through `vertices`, parameterized by arc length (0...1). */
    private fun polygon(vertices: List<P2>): (Double) -> P2 {
        val lengths = vertices.indices.map { i ->
            hypot(vertices[(i + 1) % vertices.size].x - vertices[i].x, vertices[(i + 1) % vertices.size].y - vertices[i].y)
        }
        val total = lengths.sum()
        return { f ->
            var target = f * total; var i = 0
            while (target > lengths[i] && i < vertices.size - 1) { target -= lengths[i]; i += 1 }
            val a = vertices[i]; val b = vertices[(i + 1) % vertices.size]
            a + (b - a) * (if (lengths[i] > 0) min(1.0, target / lengths[i]) else 0.0)
        }
    }
}
