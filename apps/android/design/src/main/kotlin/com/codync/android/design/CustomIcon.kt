package com.codync.android.design

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffColorFilter
import android.graphics.RectF
import android.util.Base64
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.Dp

/** PNGs are immutable; share bounded decoded images across rows, widgets and notifications. */
private val decodedIcons = object : LruCache<String, Bitmap>(4 * 1024 * 1024) {
    override fun sizeOf(key: String, value: Bitmap) = value.allocationByteCount
}

fun customIconBitmap(dataUrl: String?): Bitmap? {
    val prefix = "data:image/png;base64,"
    if (dataUrl == null || !dataUrl.startsWith(prefix) || dataUrl.length > 65_558) return null
    decodedIcons.get(dataUrl)?.let { return it }
    val bytes = try { Base64.decode(dataUrl.substring(prefix.length), Base64.NO_WRAP) }
        catch (_: IllegalArgumentException) { return null }
    if (bytes.size < 8 || !bytes.take(8).toByteArray().contentEquals(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10))) return null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    if (bounds.outWidth !in 1..256 || bounds.outHeight !in 1..256) return null
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.also { decodedIcons.put(dataUrl, it) }
}

/** A static aspect-fit bitmap suitable for system widgets and notification faces. */
fun customAvatarBitmap(dataUrl: String?, tinted: Boolean, pixels: Int, tint: Int): Bitmap? {
    require(pixels in 1..256)
    val image = customIconBitmap(dataUrl) ?: return null
    val bitmap = Bitmap.createBitmap(pixels, pixels, Bitmap.Config.ARGB_8888)
    val scale = pixels.toFloat() / maxOf(image.width, image.height)
    val width = image.width * scale
    val height = image.height * scale
    val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG).apply {
        if (tinted) colorFilter = PorterDuffColorFilter(tint, PorterDuff.Mode.SRC_IN)
    }
    Canvas(bitmap).drawBitmap(image, null, RectF((pixels - width) / 2, (pixels - height) / 2,
        (pixels + width) / 2, (pixels + height) / 2), paint)
    return bitmap
}

/** Returns false for a missing/damaged image so the caller renders its normal fallback. */
@Composable fun StaticCustomIcon(dataUrl: String?, tinted: Boolean, size: Dp, color: Color = Palette.text): Boolean {
    val bitmap = remember(dataUrl) { customIconBitmap(dataUrl)?.asImageBitmap() } ?: return false
    Image(bitmap, null, Modifier.size(size), colorFilter = if (tinted) ColorFilter.tint(color) else null)
    return true
}
