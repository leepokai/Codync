package com.codync.android.core

import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import org.bouncycastle.crypto.modes.ChaCha20Poly1305
import org.bouncycastle.crypto.params.AEADParameters
import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.params.KeyParameter
import org.bouncycastle.crypto.params.X25519PrivateKeyParameters
import org.bouncycastle.crypto.params.X25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer

/** Protocol-v1 primitives. The algorithms come from BC; labels/framing belong to Codync. */
object RelayCrypto {
    const val CHUNK_SIZE = 256 * 1024
    const val MAX_OUTBOUND = 1024 * 1024
    const val MAX_INBOUND = 16 * 1024 * 1024
    const val MAX_COUNTER = 1L shl 32
    private val random = SecureRandom()

    internal fun label(value: String): ByteArray = value.toByteArray(Charsets.UTF_8)
    internal fun randomBytes(size: Int): ByteArray = ByteArray(size).also(random::nextBytes)
    internal fun sha256(vararg parts: ByteArray): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        parts.forEach(digest::update)
        return digest.digest()
    }

    fun computerId(signKey: ByteArray): String {
        require(signKey.size == 32) { "Invalid host signing key" }
        return sha256(signKey).copyOf(16).base64Url()
    }

    fun sasCommit(deviceKey: ByteArray, deviceNonce: ByteArray): ByteArray {
        require(deviceKey.size == 32 && deviceNonce.size == 32) { "Invalid access request" }
        return sha256(label("codync/sascommit/v1"), deviceKey, deviceNonce)
    }

    fun sasCode(hostSignKey: ByteArray, deviceKey: ByteArray, deviceNonce: ByteArray, hostNonce: ByteArray): String {
        require(listOf(hostSignKey, deviceKey, deviceNonce, hostNonce).all { it.size == 32 }) { "Invalid access request" }
        val hash = sha256(label("codync/sas/v2"), hostSignKey, deviceKey, deviceNonce, hostNonce)
        val number = hash.take(4).fold(0L) { value, byte -> (value shl 8) or (byte.toLong() and 255) }
        return (number % 1_000_000).toString().padStart(6, '0')
    }

    internal fun offerId(code: ByteArray): String = sha256(label("codync/offer/v1"), code).copyOf(16).base64Url()
    internal fun signingPublic(seed: ByteArray): ByteArray = Ed25519PrivateKeyParameters(seed, 0).generatePublicKey().encoded
    internal fun agreementPublic(seed: ByteArray): ByteArray = X25519PrivateKeyParameters(seed, 0).generatePublicKey().encoded

    internal fun sign(seed: ByteArray, input: ByteArray): ByteArray {
        require(seed.size == 32)
        val signer = Ed25519Signer()
        signer.init(true, Ed25519PrivateKeyParameters(seed, 0))
        signer.update(input, 0, input.size)
        return signer.generateSignature()
    }

    internal fun verify(key: ByteArray, input: ByteArray, signature: ByteArray): Boolean {
        if (key.size != 32 || signature.size != 64) return false
        val verifier = Ed25519Signer()
        verifier.init(false, Ed25519PublicKeyParameters(key, 0))
        verifier.update(input, 0, input.size)
        return verifier.verifySignature(signature)
    }

    internal fun sharedSecret(privateKey: ByteArray, publicKey: ByteArray): ByteArray {
        require(privateKey.size == 32 && publicKey.size == 32) { "Invalid agreement key" }
        val result = ByteArray(32)
        try {
            X25519PrivateKeyParameters(privateKey, 0).generateSecret(X25519PublicKeyParameters(publicKey, 0), result, 0)
        } catch (error: IllegalStateException) {
            throw SecurityException("Invalid zero shared secret", error)
        }
        if (result.all { it == 0.toByte() }) throw SecurityException("Invalid zero shared secret")
        return result
    }

    internal fun hmac(key: ByteArray, input: ByteArray): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        return mac.doFinal(input)
    }
    internal fun extract(salt: ByteArray, secret: ByteArray): ByteArray = hmac(salt, secret)
    internal fun expand(prk: ByteArray, info: String): ByteArray = hmac(prk, label(info) + byteArrayOf(1))

    internal data class ChannelKeys(val prk: ByteArray, val d2h: ByteArray, val h2d: ByteArray)

    internal fun channelKeys(secret: ByteArray, transcript: ByteArray): ChannelKeys {
        val prk = extract(transcript, secret)
        return ChannelKeys(prk, expand(prk, "codync/d2h/v1"), expand(prk, "codync/h2d/v1"))
    }

    internal fun hs1Input(cid: ByteArray, dk: ByteArray, ekD: ByteArray, nonce: ByteArray): ByteArray =
        label("codync/hs1/v1") + cid + dk + ekD + nonce

    internal fun transcriptHash(cid: ByteArray, dk: ByteArray, ekD: ByteArray, nonce: ByteArray, ekH: ByteArray): ByteArray =
        sha256(label("codync/hs2/v1"), cid, dk, ekD, nonce, ekH)

    internal fun crypt(encrypt: Boolean, key: ByteArray, nonce: ByteArray, aad: ByteArray, input: ByteArray): ByteArray {
        val cipher = ChaCha20Poly1305()
        cipher.init(encrypt, AEADParameters(KeyParameter(key), 128, nonce, aad))
        val output = ByteArray(cipher.getOutputSize(input.size))
        try {
            val written = cipher.processBytes(input, 0, input.size, output, 0)
            val final = cipher.doFinal(output, written)
            return output.copyOf(written + final)
        } catch (error: org.bouncycastle.crypto.InvalidCipherTextException) {
            throw SecurityException("Encrypted message authentication failed", error)
        }
    }

    internal fun sealFrame(key: ByteArray, counter: Long, final: Boolean, chunk: ByteArray): ByteArray {
        require(counter in 0 until MAX_COUNTER && chunk.size <= CHUNK_SIZE)
        return crypt(true, key, ByteArray(4) + counter.bigEndian(), label("codync/frame/v1") + counter.bigEndian(),
            byteArrayOf(if (final) 0 else 1) + chunk)
    }

    internal fun openFrame(key: ByteArray, counter: Long, data: ByteArray): Pair<Boolean, ByteArray> {
        require(counter in 0 until MAX_COUNTER && data.size in 17..(CHUNK_SIZE + 17)) { "Malformed encrypted frame" }
        val plain = crypt(false, key, ByteArray(4) + counter.bigEndian(), label("codync/frame/v1") + counter.bigEndian(), data)
        require(plain[0] == 0.toByte() || plain[0] == 1.toByte()) { "Invalid chunk flag" }
        return (plain[0] == 0.toByte()) to plain.copyOfRange(1, plain.size)
    }

    internal fun openPush(sealed: String, computerId: String, pushPrivate: ByteArray): ByteArray {
        val raw = sealed.decodeBase64Url()
        require(raw.size > 48) { "Malformed sealed push" }
        val cid = computerId.decodeBase64Url(16)
        val ephemeral = raw.copyOf(32)
        val secret = sharedSecret(pushPrivate, ephemeral)
        val prk = extract(label("codync/push/v1") + cid + agreementPublic(pushPrivate) + ephemeral, secret)
        return crypt(false, expand(prk, "codync/push-key/v1"), ByteArray(12), cid, raw.copyOfRange(32, raw.size))
    }

    internal fun sealMailbox(plain: ByteArray, computer: Computer, identity: DeviceIdentity, nonce: String,
        ephemeral: ByteArray = randomBytes(32)): String {
        require(plain.size <= 64 * 1024 - 112 && nonce.isNotEmpty()) { "Message is too large for the offline mailbox." }
        val cid = computer.id.decodeBase64Url(16)
        val public = agreementPublic(ephemeral)
        val secret = sharedSecret(ephemeral, requireNotNull(computer.boxKey).decodeBase64Url(32))
        val prk = extract(label("codync/mbox/v1") + cid + identity.deviceKey + public, secret)
        val cipher = crypt(true, expand(prk, "codync/mbox-key/v1"), ByteArray(12), identity.deviceKey + label(nonce), plain)
        val signature = identity.sign(label("codync/mbox/v1") + cid + public + cipher)
        return (public + signature + cipher).base64Url()
    }
}
