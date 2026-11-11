package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

/**
 * The cloud's word, signed for one computer, that this device's key belongs to the account
 * (iOS `DeviceCertificate`): what pairing shows a computer before it lets the phone in.
 */
@Serializable
data class DeviceCertificate(val payload: String, val sig: String) {
    /** When computers stop taking it; 0 when the payload can't be read. */
    val exp: Long get() = runCatching { Json.parseToJsonElement(payload).jsonObject.getValue("exp").jsonPrimitive.long }.getOrDefault(0)

    fun isValid(now: Long = System.currentTimeMillis()): Boolean = exp > now

    /** Valid for more than another day: reused rather than asked for again. */
    fun isFresh(now: Long = System.currentTimeMillis()): Boolean = isValid(now + DAY)

    companion object {
        private const val DAY = 86_400_000L
    }
}

/** A claim the account opened (`POST /v1/claims`), for a computer in no account to sign while pairing. */
@Serializable
data class CloudClaim(val claimId: String, val nonce: String)
