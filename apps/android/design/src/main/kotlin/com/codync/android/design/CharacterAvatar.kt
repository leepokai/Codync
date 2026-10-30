package com.codync.android.design

import android.animation.ValueAnimator
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Region
import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlin.math.*

object AvatarPalette {
    val colors = linkedMapOf("black" to 0xFF2B2B2B, "brown" to 0xFF936439, "red" to 0xFFFF263C,
        "orange" to 0xFFFF6700, "yellow" to 0xFFFF9800, "green" to 0xFF00C972, "cyan" to 0xFF00BCA6,
        "blue" to 0xFF1084FE, "violet" to 0xFF9159FE, "magenta" to 0xFFFF309B, "gray" to 0xFF777777)
    val shapes = listOf("blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop")
    fun tint(id: String): Color = Color(colors[id] ?: colors.getValue("blue"))
}

data class AvatarFace(val shape: String, val color: String, val status: String = "idle", val customImage: String? = null, val customTinted: Boolean = false)

/** Static system-widget drawing uses the same mask, palette and eyes as the app; [cells] is its grid (7, 9 or 13). */
fun characterAvatarBitmap(shape: String, color: String, pixels: Int, ink: Int, highlight: Int? = null, cells: Int = 9): android.graphics.Bitmap {
    require(pixels in 1..256 && cells in listOf(7, 9, 13))
    val bitmap = android.graphics.Bitmap.createBitmap(pixels, pixels, android.graphics.Bitmap.Config.ARGB_8888)
    val canvas = android.graphics.Canvas(bitmap)
    val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG)
    val step = pixels / cells.toFloat()
    val half = pixels / 2.0
    val lx = sin(-.7) * .8; val ly = .55; val lz = cos(-.7) * .5 + .6
    val ll = sqrt(lx * lx + ly * ly + lz * lz)
    for ((row, col) in avatarDots(shape, cells)) {
        if (col in eyeColumns(cells) && row in eyeRows(cells)) continue
        val x = (col + .5f) * step; val y = (row + .5f) * step
        val u = (x - half) / half; val v = (half - y) / half
        val z = sqrt(max(.2, 1.0 - u * u - v * v))
        val shade = .3 + .7 * max(0.0, (u * lx + v * ly + z * lz) / (sqrt(u * u + v * v + z * z) * ll))
        val radius = (step * .42 * (.55 + .45 * shade)).toFloat()
        paint.color = ink; paint.alpha = (dotInk(cells, shade) * 255).roundToInt().coerceIn(0, 255)
        canvas.drawCircle(x, y, radius, paint)
        if (shade > .6) {
            paint.color = highlight ?: (AvatarPalette.colors[color] ?: AvatarPalette.colors.getValue("blue")).toInt()
            paint.alpha = (((shade - .6) / .4) * 255).roundToInt().coerceIn(0, 255)
            canvas.drawCircle(x, y, radius, paint)
        }
    }
    return bitmap
}

/** Port of CodyncKit's dotted character; the accompanying name/status supplies accessibility text. */
@Composable fun CharacterAvatar(shape: String, color: String, size: Dp = 40.dp, status: String = "idle", monochrome: Color? = null,
    highlight: Color? = null, customImage: String? = null, customTinted: Boolean = false) {
    if (StaticCustomIcon(customImage, customTinted, size, monochrome ?: AvatarPalette.tint(color))) return
    val owner = LocalLifecycleOwner.current
    var active by remember(owner) { mutableStateOf(owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, _ -> active = owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    val time = if (customImage == null && active && status in listOf("working", "needsInput") && ValueAnimator.areAnimatorsEnabled()) {
        val transition = rememberInfiniteTransition(label = "Character activity")
        val seconds by transition.animateFloat(0f, 3600f, infiniteRepeatable(tween(3_600_000, easing = LinearEasing)), label = "Character light and eyes")
        seconds.toDouble()
    } else 0.0
    val cells = if (size < 18.dp) 7 else if (size < 28.dp) 9 else 13
    val dots = remember(shape, cells) { avatarDots(shape, cells) }
    val ink = monochrome ?: Palette.text
    val tint = monochrome ?: highlight ?: AvatarPalette.tint(color)
    Canvas(Modifier.size(size).clearAndSetSemantics { }) {
        val step = this.size.width / cells
        val half = this.size.width / 2
        val glance = if (status == "working" && time > 0) (sin(time * 2 * PI / 3.2) * 1.4).roundToInt() else 0
        val eyes = eyeColumns(cells).map { it + glance }
        val eyeRows = eyeRows(cells)
        val blinking = time > 0 && (time / 4.7) % 1 < 0.035
        val yaw = if (status == "working" && time > 0) time * 1.4 else -0.7
        val lx = sin(yaw) * .8; val ly = .55; val lz = cos(yaw) * .5 + .6
        val ll = sqrt(lx * lx + ly * ly + lz * lz)
        for ((row, col) in dots) {
            if (col in eyes && row in if (blinking) eyeRows.takeLast(1) else eyeRows) continue
            val center = Offset((col + .5f) * step, (row + .5f) * step)
            val u = (center.x - half) / half; val v = (half - center.y) / half
            val z = sqrt(max(.2, 1.0 - u * u - v * v))
            val nl = sqrt(u * u + v * v + z * z)
            var shade = .3 + .7 * max(0.0, (u * lx + v * ly + z * lz) / (nl * ll))
            if (status == "needsInput") shade *= .6 + .4 * (.5 + .5 * sin(sqrt((u * u + v * v).toDouble()) * 9 - time * 5))
            val radius = (step * .42 * (.55 + .45 * shade)).toFloat()
            drawCircle(ink.copy(alpha = dotInk(cells, shade).toFloat()), radius, center)
            if (shade > .6) drawCircle(tint.copy(alpha = ((shade - .6) / .4).toFloat().coerceIn(0f, 1f)), radius, center)
        }
    }
}

@Composable fun GroupAvatar(faces: List<AvatarFace>, size: Dp = 40.dp) {
    Box(Modifier.size(size).clearAndSetSemantics { }) {
        fun side(fraction: Float) = size * fraction
        @Composable fun place(index: Int, fraction: Float, align: Alignment) {
            val face = faces[index]
            Box(Modifier.align(align)) { CharacterAvatar(face.shape, face.color, side(fraction), face.status, customImage = face.customImage, customTinted = face.customTinted) }
        }
        when (faces.size) {
            0 -> Symbol("person.2", Modifier.align(Alignment.Center), size = size * 0.4f, tint = Palette.secondary)
            1 -> place(0, 1f, Alignment.Center)
            2 -> { place(1, .66f, Alignment.TopEnd); place(0, .66f, Alignment.BottomStart) }
            3 -> { place(0, .55f, Alignment.TopCenter); place(1, .55f, Alignment.BottomStart); place(2, .55f, Alignment.BottomEnd) }
            else -> {
                place(0, .5f, Alignment.TopStart); place(1, .5f, Alignment.TopEnd); place(2, .5f, Alignment.BottomStart)
                if (faces.size == 4) place(3, .5f, Alignment.BottomEnd)
                else Box(Modifier.size(size * .5f).align(Alignment.BottomEnd), contentAlignment = Alignment.Center) {
                    Text("+${faces.size - 3}", style = Type.system(size.value * 0.24f, FontWeight.Bold), color = Palette.secondary)
                }
            }
        }
    }
}

private fun eyeColumns(cells: Int) = when (cells) { 7 -> listOf(2, 4); 9 -> listOf(3, 6); else -> listOf(4, 8) }
private fun eyeRows(cells: Int) = when (cells) { 7 -> listOf(2, 3); 9 -> listOf(3, 4); else -> listOf(4, 5, 6) }
/** A dot's ink opacity for its shade: finer grids fade their dim side further. */
private fun dotInk(cells: Int, shade: Double) = if (cells < 13) .4 + .4 * shade else .2 + .4 * min(1.0, shade / .7)

/** Integer mask is computed once per shape/grid, never in the animation loop. */
private fun avatarDots(shape: String, cells: Int): List<Pair<Int, Int>> {
    val path = avatarPath(shape)
    val region = Region().apply { setPath(path, Region(-100, -100, 1100, 1100)) }
    return buildList { for (row in 0 until cells) for (col in 0 until cells)
        if (region.contains(((col + .5) * 1000 / cells).toInt(), ((row + .5) * 1000 / cells).toInt())) add(row to col) }
}

private fun avatarPath(shape: String): Path = Path().apply {
    fillType = Path.FillType.WINDING
    when (shape) {
        "pebble" -> addOval(RectF(0f, 100f, 1000f, 900f), Path.Direction.CW)
        "squircle" -> addRoundRect(RectF(40f, 40f, 960f, 960f), 300f, 300f, Path.Direction.CW)
        "tablet" -> addRoundRect(RectF(140f, 0f, 860f, 1000f), 220f, 220f, Path.Direction.CW)
        "wedge" -> { moveTo(500f, 40f); quadTo(900f, 400f, 980f, 860f); quadTo(500f, 1040f, 20f, 860f); quadTo(100f, 400f, 500f, 40f); close() }
        "hex" -> {
            for (i in 0 until 6) {
                val angle = i * PI / 3 - PI / 2
                val x = (500 + cos(angle) * 490).toFloat(); val y = (500 + sin(angle) * 490).toFloat()
                if (i == 0) moveTo(x, y) else lineTo(x, y)
            }
            close()
            val stroke = Path()
            android.graphics.Paint().apply { style = android.graphics.Paint.Style.STROKE; strokeWidth = 80f; strokeJoin = android.graphics.Paint.Join.ROUND }.getFillPath(this, stroke)
            op(stroke, Path.Op.UNION)
        }
        "cloud" -> {
            addOval(RectF(0f, 300f, 550f, 850f), Path.Direction.CW)
            addOval(RectF(450f, 300f, 1000f, 850f), Path.Direction.CW)
            addOval(RectF(180f, 80f, 820f, 720f), Path.Direction.CW)
            addRoundRect(RectF(100f, 500f, 900f, 850f), 150f, 150f, Path.Direction.CW)
        }
        "teardrop" -> { moveTo(500f, 0f); cubicTo(620f, 200f, 940f, 380f, 940f, 620f)
            arcTo(RectF(60f, 180f, 940f, 1060f), 0f, 180f); cubicTo(60f, 380f, 380f, 200f, 500f, 0f); close() }
        else -> { for (i in 0..64) {
            val angle = i / 64.0 * 2 * PI; val radius = .46 + .035 * sin(angle * 3 + .6)
            val x = (500 + cos(angle) * 1000 * radius).toFloat(); val y = (500 + sin(angle) * 1000 * radius).toFloat()
            if (i == 0) moveTo(x, y) else lineTo(x, y)
        }; close() }
    }
}
