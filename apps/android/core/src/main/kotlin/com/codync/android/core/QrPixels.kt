package com.codync.android.core

import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter

/** A QR code for [text] as black and white ARGB pixels, [size] on each side; the apps turn them into an image. */
object QrPixels {
    fun encode(text: String, size: Int): IntArray {
        require(size > 0) { "the code needs a size" }
        val matrix = QRCodeWriter().encode(
            text,
            BarcodeFormat.QR_CODE,
            size,
            size,
            mapOf(EncodeHintType.CHARACTER_SET to "UTF-8"),
        )
        return IntArray(size * size) { index ->
            if (matrix.get(index % size, index / size)) 0xFF000000.toInt() else 0xFFFFFFFF.toInt()
        }
    }
}
