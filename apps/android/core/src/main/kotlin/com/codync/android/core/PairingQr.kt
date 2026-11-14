package com.codync.android.core

import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import java.nio.ByteBuffer

sealed interface PairingScan {
    data class Found(val link: String) : PairingScan
    data class Invalid(val message: String) : PairingScan
}

/** Bundled decoding works without Play services, network access or a model download. */
class PairingQr(private val scheme: String) {
    private val reader = QRCodeReader()
    @Synchronized fun decode(pixels: ByteArray, width: Int, height: Int): PairingScan? {
        require(width > 0 && height > 0 && width.toLong() * height <= MAX_PIXELS && pixels.size.toLong() == width.toLong() * height)
        val source = PlanarYUVLuminanceSource(pixels, width, height, 0, 0, width, height, false)
        for (luminance in listOf(source, source.invert())) {
            try {
                val result = reader.decode(BinaryBitmap(HybridBinarizer(luminance)), mapOf(DecodeHintType.TRY_HARDER to true))
                val value = result.text
                if (!value.startsWith("$scheme://pair?")) return null
                try { Pairing.parse(value, scheme); return PairingScan.Found(value) }
                catch (failure: IllegalArgumentException) { return PairingScan.Invalid(failure.message ?: "Show a fresh Codync pairing code on your computer.") }
            } catch (_: ReaderException) { }
            finally { reader.reset() }
        }
        return null
    }

    companion object {
        private const val MAX_PIXELS = 4L * 1024 * 1024
        /** Camera Y planes can have padding, a position offset, and non-unit pixel strides. */
        fun luminance(buffer: ByteBuffer, width: Int, height: Int, rowStride: Int, pixelStride: Int): ByteArray {
            require(width > 0 && height > 0 && width.toLong() * height <= MAX_PIXELS && pixelStride > 0)
            val lastPixel = (width - 1L) * pixelStride
            require(rowStride.toLong() > lastPixel)
            val required = (height - 1L) * rowStride + lastPixel + 1
            require(required <= buffer.remaining()) { "Incomplete camera image" }
            val source = buffer.duplicate()
            val base = source.position()
            return ByteArray(width * height) { index -> source.get(base + index / width * rowStride + index % width * pixelStride) }
        }
    }
}
