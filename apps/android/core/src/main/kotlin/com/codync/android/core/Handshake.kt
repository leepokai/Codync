package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class Handshake(
    computer: Computer,
    private val identity: DeviceIdentity,
    private val ephemeral: ByteArray = RelayCrypto.randomBytes(32),
    private val nonce: ByteArray = RelayCrypto.randomBytes(32),
) {
    private val hostKey = computer.signKey.decodeBase64Url(32)
    private val cid = computer.id.decodeBase64Url(16)
    private val ekD = RelayCrypto.agreementPublic(ephemeral)
    init {
        computer.validate()
        require(ephemeral.size == 32 && nonce.size == 32)
    }
    fun hello(pair: Boolean): JsonObject = buildJsonObject {
        put("t", "hello")
        put("v", 1)
        put("dk", identity.publicKey)
        put("ek", ekD.base64Url())
        put("n", nonce.base64Url())
        put("sig", identity.sign(RelayCrypto.hs1Input(cid, identity.deviceKey, ekD, nonce)).base64Url())
        if (pair) put("pair", true)
    }
    fun finish(ekH: ByteArray, signature: ByteArray): RelayCrypto.ChannelKeys {
        require(ekH.size == 32 && signature.size == 64)
        val transcript = RelayCrypto.transcriptHash(cid, identity.deviceKey, ekD, nonce, ekH)
        if (!RelayCrypto.verify(hostKey, transcript, signature)) throw SecurityException("This computer's identity changed. Pair it again.")
        return RelayCrypto.channelKeys(RelayCrypto.sharedSecret(ephemeral, ekH), transcript)
    }
}

