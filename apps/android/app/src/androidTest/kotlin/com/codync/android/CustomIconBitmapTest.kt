package com.codync.android

import android.graphics.Bitmap
import android.graphics.Color
import android.util.Base64
import com.codync.android.design.customAvatarBitmap
import com.codync.android.design.customIconBitmap
import java.io.ByteArrayOutputStream
import org.junit.Assert.*
import org.junit.Test

class CustomIconBitmapTest {
    private fun png(width: Int, height: Int): String {
        val image = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        val bytes = ByteArrayOutputStream().use { output ->
            image.compress(Bitmap.CompressFormat.PNG, 100, output)
            output.toByteArray()
        }
        image.recycle()
        return "data:image/png;base64," + Base64.encodeToString(bytes, Base64.NO_WRAP)
    }

    @Test fun originalAndTintedImagesKeepAspectRatioAndShareDecodedPixels() {
        val source = png(2, 1)
        assertSame(customIconBitmap(source), customIconBitmap(source))
        val original = requireNotNull(customAvatarBitmap(source, false, 16, Color.BLUE))
        val tinted = requireNotNull(customAvatarBitmap(source, true, 16, Color.BLUE))
        assertEquals(Color.RED, original.getPixel(8, 8))
        assertEquals(Color.BLUE, tinted.getPixel(8, 8))
        assertEquals(0, Color.alpha(original.getPixel(8, 0)))
        assertEquals(0, Color.alpha(tinted.getPixel(8, 0)))
    }

    @Test fun malformedAndOversizedImagesUseTheBuiltInFallback() {
        for (source in listOf("https://example.com/image.png", "data:image/png;base64,iVBORw0KGgo=", png(257, 1))) {
            assertNull(customIconBitmap(source))
            assertNull(customAvatarBitmap(source, false, 32, Color.BLUE))
        }
    }
}
