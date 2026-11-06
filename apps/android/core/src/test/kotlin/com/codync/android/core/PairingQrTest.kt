package com.codync.android.core

import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import java.net.URLEncoder
import java.nio.ByteBuffer
import org.junit.Assert.*
import org.junit.Test

class PairingQrTest {
    private fun code(): String {
        val keys = vectors().getValue("keys") as kotlinx.serialization.json.JsonObject
        val fields = mapOf("v" to "3", "id" to requireNotNull(keys.text("computerId")), "name" to "Acceptance computer",
            "sk" to requireNotNull(keys.text("hostSignPub")), "bk" to requireNotNull(keys.text("hostBoxPub")),
            "code" to ByteArray(16) { 8 }.base64Url(), "urls" to "http://192.168.1.2:19222")
        return "codync://pair?" + fields.entries.joinToString("&") { "${it.key}=${URLEncoder.encode(it.value, "UTF-8")}" }
    }
    private fun pixels(value: String, size: Int = 512): ByteArray {
        val matrix = QRCodeWriter().encode(value, BarcodeFormat.QR_CODE, size, size, mapOf(EncodeHintType.CHARACTER_SET to "UTF-8"))
        return ByteArray(size * size) { if (matrix[it % size, it / size]) 0 else 255.toByte() }
    }

    @Test fun `bundled QR decoding accepts every rotation mirrored and inverted pairing images`() {
        val value = code(); val decoder = PairingQr("codync"); var image = pixels(value)
        repeat(4) {
            assertEquals(PairingScan.Found(value), decoder.decode(image, 512, 512))
            image = ByteArray(image.size) { index -> image[(511 - index % 512) * 512 + index / 512] }
        }
        assertEquals(PairingScan.Found(value), decoder.decode(ByteArray(image.size) { image[it / 512 * 512 + 511 - it % 512] }, 512, 512))
        assertEquals(PairingScan.Found(value), decoder.decode(ByteArray(image.size) { (255 - (image[it].toInt() and 255)).toByte() }, 512, 512))
    }

    @Test fun `unrelated obsolete and malformed QR codes never become a pairing request`() {
        val decoder = PairingQr("codync")
        assertNull(decoder.decode(pixels("https://example.test"), 512, 512))
        for (value in listOf("codync://pair?v=2", code() + "&code=duplicate")) assertTrue(decoder.decode(pixels(value), 512, 512) is PairingScan.Invalid)
        val obsolete = decoder.decode(pixels("codync://pair?v=2"), 512, 512) as PairingScan.Invalid
        assertTrue(obsolete.message.contains("Update Codync"))
        assertNull(decoder.decode(ByteArray(128 * 128) { 127 }, 128, 128))
    }

    @Test fun `padded strided image planes preserve pixels and the caller buffer position`() {
        val raw = ByteArray(36) { 99 }; raw[5] = 1; raw[7] = 2; raw[9] = 3; raw[17] = 4; raw[19] = 5; raw[21] = 6
        val buffer = ByteBuffer.wrap(raw).apply { position(5); limit(22) }
        assertArrayEquals(byteArrayOf(1, 2, 3, 4, 5, 6), PairingQr.luminance(buffer, 3, 2, 12, 2))
        assertEquals(5, buffer.position()); assertEquals(22, buffer.limit())
        assertFails<IllegalArgumentException> { PairingQr.luminance(buffer, 4, 2, 12, 2) }
        assertFails<IllegalArgumentException> { PairingQr.luminance(buffer, 3, 2, 2, 2) }
        assertFails<IllegalArgumentException> { PairingQr.luminance(buffer, Int.MAX_VALUE, Int.MAX_VALUE, 12, 1) }
    }
}
