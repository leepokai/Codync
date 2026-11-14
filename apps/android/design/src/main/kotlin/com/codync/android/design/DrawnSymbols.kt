package com.codync.android.design

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.painter.Painter

/**
 * The tab bar's two symbols, drawn here because no icon set has their shapes: overlapping speech
 * bubbles and a stack of cards. Original drawings in each symbol's proportions.
 */
internal class DrawnSymbol(private val width: Float, private val height: Float, private val shape: () -> Path) : Painter() {
    private var tint: ColorFilter? = null
    private val path by lazy(shape)

    override val intrinsicSize = Size.Unspecified

    override fun applyColorFilter(colorFilter: ColorFilter?): Boolean { tint = colorFilter; return true }

    override fun DrawScope.onDraw() {
        val unit = minOf(size.width / width, size.height / height)
        val dx = (size.width - width * unit) / 2; val dy = (size.height - height * unit) / 2
        translate(dx, dy) { scale(unit, unit, Offset.Zero) { drawPath(path, Color.Black, colorFilter = tint) } }
    }
}

private fun round(left: Float, top: Float, right: Float, bottom: Float, radius: Float, bottomRadius: Float = radius) = Path().apply {
    addRoundRect(RoundRect(Rect(left, top, right, bottom), CornerRadius(radius), CornerRadius(radius),
        CornerRadius(bottomRadius), CornerRadius(bottomRadius)))
}

private fun tail(vararg points: Float) = Path().apply {
    moveTo(points[0], points[1])
    for (i in 2 until points.size step 2) lineTo(points[i], points[i + 1])
    close()
}

private fun union(a: Path, b: Path) = Path().apply { op(a, b, PathOperation.Union) }

/** `bubble.left.and.bubble.right.fill`: the right bubble in front, cut out of the left one. */
internal val Bubbles = DrawnSymbol(34.5f, 27.5f) {
    val back = union(round(1f, 1f, 26f, 19.8f, 6f), tail(6.6f, 18f, 6.6f, 23.9f, 7.6f, 24.4f, 12.6f, 18f))
    val front = union(round(12.8f, 6.9f, 33.5f, 22.6f, 5.5f), tail(21.6f, 22f, 27f, 26.6f, 28.2f, 26f, 28.2f, 22f))
    val gap = union(round(11.3f, 5.4f, 35f, 24.1f, 7f), tail(20.4f, 23f, 27f, 28.4f, 29.7f, 27f, 29.7f, 23f))
    union(Path().apply { op(back, gap, PathOperation.Difference) }, front)
}

/** `rectangle.stack.fill`: a card with two narrower ones peeking out above it. */
internal val CardStack = DrawnSymbol(26f, 27.4f) {
    val stack = union(round(1f, 7.2f, 25f, 26.4f, 4.6f), round(3.7f, 3.9f, 22.3f, 5.8f, 1.7f, 0.3f))
    union(stack, round(6f, 1f, 20f, 2.6f, 1.3f, 0.3f))
}
