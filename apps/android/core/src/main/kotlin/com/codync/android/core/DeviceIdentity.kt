package com.codync.android.core

/** Storage owns the private seeds; the channel never serializes or logs them. */
class DeviceIdentity(signingSeed: ByteArray, pushSeed: ByteArray) {
    private val signingSeed = signingSeed.copyOf()
    private val pushSeed = pushSeed.copyOf()
    init {
        require(signingSeed.size == 32 && pushSeed.size == 32)
    }
    val publicKey: String get() = RelayCrypto.signingPublic(signingSeed).base64Url()
    val pushKey: String get() = RelayCrypto.agreementPublic(pushSeed).base64Url()
    internal val deviceKey: ByteArray get() = RelayCrypto.signingPublic(signingSeed)
    internal fun sign(input: ByteArray): ByteArray = RelayCrypto.sign(signingSeed, input)
    fun openPush(sealed: String, computerId: String): ByteArray = RelayCrypto.openPush(sealed, computerId, pushSeed)
}

