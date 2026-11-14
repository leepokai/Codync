package com.codync.android.core

import java.nio.ByteBuffer
import java.util.Base64

internal fun ByteArray.base64Url(): String = Base64.getUrlEncoder().withoutPadding().encodeToString(this)

internal fun String.decodeBase64Url(size: Int? = null): ByteArray {
    require(length % 4 != 1 && all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '-' || it == '_' }) {
        "Invalid base64url value"
    }
    val bytes = Base64.getUrlDecoder().decode(this)
    require(bytes.base64Url() == this && (size == null || bytes.size == size)) { "Invalid base64url length or encoding" }
    return bytes
}

internal fun Long.bigEndian(): ByteArray = ByteBuffer.allocate(8).putLong(this).array()

