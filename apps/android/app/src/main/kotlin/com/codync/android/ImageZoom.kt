package com.codync.android

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size

/**
 * Pinch-to-zoom for a picture shown in a box (desktop `lib/image-zoom.ts`): how big it is, from 1 (fit) to
 * [MAX_SCALE], and how far it is moved from the box's center, in the box's pixels. Points are from the box's center;
 * `picture` is the picture's size at fit.
 */
internal data class ImageZoom(val scale: Float = 1f, val offset: Offset = Offset.Zero) {
    val zoomedIn: Boolean get() = scale > 1f

    /** The picture stays inside the box when it is smaller than it, and never leaves a gap at an edge when it is larger. */
    fun clamped(box: Size, picture: Size): ImageZoom {
        val scale = scale.coerceIn(1f, MAX_SCALE)
        fun reach(boxSide: Float, pictureSide: Float) = maxOf(0f, (pictureSide * scale - boxSide) / 2)
        val (x, y) = reach(box.width, picture.width) to reach(box.height, picture.height)
        // `+ 0f` turns a -0 into 0.
        return ImageZoom(scale, Offset(offset.x.coerceIn(-x, x) + 0f, offset.y.coerceIn(-y, y) + 0f))
    }

    /** Zooms by `factor` keeping the spot under `point` where it is. */
    fun zoomedBy(factor: Float, point: Offset, box: Size, picture: Size): ImageZoom {
        val scale = (scale * factor).coerceIn(1f, MAX_SCALE)
        return ImageZoom(scale, point - (point - offset) * (scale / this.scale)).clamped(box, picture)
    }

    fun pannedBy(delta: Offset, box: Size, picture: Size): ImageZoom = copy(offset = offset + delta).clamped(box, picture)

    /** Where a double tap goes: back to fit when zoomed in, else in on the spot tapped. */
    fun toggled(point: Offset, box: Size, picture: Size): ImageZoom =
        if (zoomedIn) Fit else zoomedBy(DOUBLE_TAP_SCALE, point, box, picture)

    /** `fraction` of the way to [to], for an animation between two zooms. */
    fun lerp(to: ImageZoom, fraction: Float): ImageZoom =
        ImageZoom(scale + (to.scale - scale) * fraction, offset + (to.offset - offset) * fraction)

    companion object {
        val Fit = ImageZoom()
        const val MAX_SCALE = 8f
        const val DOUBLE_TAP_SCALE = 2.5f

        /** A picture of `size` fitted into `box`, keeping its aspect ratio (`ContentScale.Fit`, which also enlarges a small one). */
        fun fitted(size: Size, box: Size): Size {
            if (size.width <= 0f || size.height <= 0f) return box
            val ratio = minOf(box.width / size.width, box.height / size.height)
            return Size(size.width * ratio, size.height * ratio)
        }
    }
}
