package com.codync.android.core

import okhttp3.HttpUrl

internal object RequestSigner {
    fun header(identity: DeviceIdentity, method: String, url: HttpUrl, body: ByteArray = byteArrayOf(),
        timestamp: Long = System.currentTimeMillis(), nonce: ByteArray = RelayCrypto.randomBytes(16)): String {
        val authority = url.toUri().rawAuthority.lowercase()
        val path = url.encodedPath + (url.encodedQuery?.let { "?$it" } ?: "")
        val canonical = listOf("codync-sig-v1", method.uppercase(), authority, path, timestamp.toString(),
            nonce.base64Url(), RelayCrypto.sha256(body).base64Url()).joinToString("\n")
        val signature = identity.sign(canonical.toByteArray()).base64Url()
        return "v=1,kid=${identity.publicKey},ts=$timestamp,nonce=${nonce.base64Url()},sig=$signature"
    }
}
