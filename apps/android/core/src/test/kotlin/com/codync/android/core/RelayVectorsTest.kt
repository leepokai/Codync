package com.codync.android.core

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

internal fun vectors(): JsonObject = Json.parseToJsonElement(File(System.getProperty("codync.vectors")).readText()).jsonObject
internal fun JsonObject.bytes(key: String): ByteArray = getValue(key).jsonPrimitive.content.decodeBase64Url()

class RelayVectorsTest {
    private val fixture = vectors()
    private val keys = fixture.getValue("keys").jsonObject
    private val hs = fixture.getValue("handshake").jsonObject

    @Test fun `keys and identifiers match the shared vectors`() {
        assertArrayEquals(keys.bytes("hostSignPub"), RelayCrypto.signingPublic(keys.bytes("hostSignSeed")))
        assertArrayEquals(keys.bytes("deviceSignPub"), RelayCrypto.signingPublic(keys.bytes("deviceSignSeed")))
        assertArrayEquals(keys.bytes("hostBoxPub"), RelayCrypto.agreementPublic(keys.bytes("hostBoxPriv")))
        assertEquals(keys.text("computerId"), RelayCrypto.computerId(keys.bytes("hostSignPub")))
        val pairing = fixture.getValue("pairing").jsonObject
        assertEquals(pairing.text("offerId"), RelayCrypto.offerId(pairing.bytes("code")))
    }

    @Test fun `handshake signatures and both channel keys match upstream`() {
        val cid = requireNotNull(keys.text("computerId")).decodeBase64Url(16)
        val input = RelayCrypto.hs1Input(cid, keys.bytes("deviceSignPub"), hs.bytes("deviceEphPub"), hs.bytes("n"))
        assertArrayEquals(hs.bytes("hs1SignInput"), input)
        assertArrayEquals(hs.bytes("hs1Sig"), RelayCrypto.sign(keys.bytes("deviceSignSeed"), input))
        val transcript = RelayCrypto.transcriptHash(cid, keys.bytes("deviceSignPub"), hs.bytes("deviceEphPub"), hs.bytes("n"), hs.bytes("hostEphPub"))
        assertArrayEquals(hs.bytes("transcriptHash"), transcript)
        assertArrayEquals(hs.bytes("hs2Sig"), RelayCrypto.sign(keys.bytes("hostSignSeed"), transcript))
        assertTrue(RelayCrypto.verify(keys.bytes("hostSignPub"), transcript, hs.bytes("hs2Sig")))
        val secret = RelayCrypto.sharedSecret(hs.bytes("deviceEphPriv"), hs.bytes("hostEphPub"))
        assertArrayEquals(hs.bytes("sharedSecret"), secret)
        val derived = RelayCrypto.channelKeys(secret, transcript)
        assertArrayEquals(hs.bytes("prk"), derived.prk)
        assertArrayEquals(hs.bytes("kD2H"), derived.d2h)
        assertArrayEquals(hs.bytes("kH2D"), derived.h2d)
        val identity = DeviceIdentity(keys.bytes("deviceSignSeed"), fixture.getValue("push").jsonObject.bytes("pushPriv"))
        val computer = Computer(requireNotNull(keys.text("computerId")), "Fixture", requireNotNull(keys.text("hostSignPub")))
        val handshake = Handshake(computer, identity, hs.bytes("deviceEphPriv"), hs.bytes("n"))
        assertEquals(hs.text("hs1Sig"), handshake.hello(false).text("sig"))
        val actual = handshake.finish(hs.bytes("hostEphPub"), hs.bytes("hs2Sig"))
        assertArrayEquals(derived.h2d, actual.h2d)
    }

    @Test fun `every upstream encrypted frame is byte identical`() {
        fixture.getValue("frames").jsonArray.forEach { element ->
            val frame = element.jsonObject
            val key = hs.bytes(if (frame.text("dir") == "d2h") "kD2H" else "kH2D")
            val counter = frame.getValue("c").jsonPrimitive.long
            val final = frame.getValue("final").jsonPrimitive.boolean
            val message = requireNotNull(frame.text("message")).toByteArray()
            val encrypted = RelayCrypto.sealFrame(key, counter, final, message)
            assertArrayEquals(frame.bytes("d"), encrypted)
            val opened = RelayCrypto.openFrame(key, counter, encrypted)
            assertEquals(final, opened.first)
            assertArrayEquals(message, opened.second)
        }
    }

    @Test fun `signed HTTP requests match the canonical authority path and body bytes`() {
        val request = fixture.getValue("requestSig").jsonObject
        val identity = DeviceIdentity(keys.bytes("deviceSignSeed"), ByteArray(32) { 12 })
        val url = ("https://" + request.text("authority") + request.text("pathAndQuery")).toHttpUrl()
        val header = RequestSigner.header(identity, requireNotNull(request.text("method")), url,
            timestamp = request.getValue("ts").jsonPrimitive.long, nonce = request.bytes("nonce"))
        assertEquals(request.text("header"), "Codync-Sig: $header")
        val changed = RequestSigner.header(identity, "GET", url, byteArrayOf(1), request.getValue("ts").jsonPrimitive.long, request.bytes("nonce"))
        assertFalse(header == changed)
    }

    @Test fun `sealed notifications open with the existing push identity`() {
        val push = fixture.getValue("push").jsonObject
        val opened = RelayCrypto.openPush(requireNotNull(push.text("sealed")), requireNotNull(keys.text("computerId")), push.bytes("pushPriv"))
        assertEquals(push.text("plaintext"), opened.toString(Charsets.UTF_8))
    }

    @Test fun `account commit and six digit code match upstream SAS vectors`() {
        val sas = fixture.getValue("sas").jsonObject
        assertArrayEquals(sas.bytes("commit"), RelayCrypto.sasCommit(keys.bytes("deviceSignPub"), sas.bytes("deviceNonce")))
        assertEquals(sas.text("code"), RelayCrypto.sasCode(keys.bytes("hostSignPub"), keys.bytes("deviceSignPub"),
            sas.bytes("deviceNonce"), sas.bytes("hostNonce")))
        assertFails<IllegalArgumentException> { RelayCrypto.sasCode(ByteArray(31), ByteArray(32), ByteArray(32), ByteArray(32)) }
    }

    @Test fun `mailbox key derivation remains compatible with the shared vector`() {
        val mailbox = fixture.getValue("mailbox").jsonObject
        val secret = RelayCrypto.sharedSecret(mailbox.bytes("ephPriv"), keys.bytes("hostBoxPub"))
        assertArrayEquals(mailbox.bytes("sharedSecret"), secret)
        val cid = requireNotNull(keys.text("computerId")).decodeBase64Url(16)
        val dk = keys.bytes("deviceSignPub")
        val ephemeral = mailbox.bytes("ephPub")
        val prk = RelayCrypto.extract(RelayCrypto.label("codync/mbox/v1") + cid + dk + ephemeral, secret)
        val key = RelayCrypto.expand(prk, "codync/mbox-key/v1")
        assertArrayEquals(mailbox.bytes("key"), key)
        val plain = requireNotNull(mailbox.text("plaintext")).toByteArray()
        val ct = RelayCrypto.crypt(true, key, ByteArray(12), dk + requireNotNull(mailbox.text("clientNonce")).toByteArray(), plain)
        assertArrayEquals(mailbox.bytes("ciphertext"), ct)
        assertArrayEquals(mailbox.bytes("sig"), RelayCrypto.sign(keys.bytes("deviceSignSeed"), RelayCrypto.label("codync/mbox/v1") + cid + ephemeral + ct))
        val identity = DeviceIdentity(keys.bytes("deviceSignSeed"), ByteArray(32) { 12 })
        val computer = Computer(requireNotNull(keys.text("computerId")), "Fixture", requireNotNull(keys.text("hostSignPub")),
            boxKey = requireNotNull(keys.text("hostBoxPub")))
        val sealed = RelayCrypto.sealMailbox(plain, computer, identity, requireNotNull(mailbox.text("clientNonce")), mailbox.bytes("ephPriv"))
        assertEquals(mailbox.text("blob"), sealed)
        assertFalse(RelayCrypto.sealMailbox(plain, computer, identity, "new-attempt") == RelayCrypto.sealMailbox(plain, computer, identity, "new-attempt"))
    }

    @Test fun `all zero agreement keys and forged host signatures are rejected`() {
        assertFails<SecurityException> { RelayCrypto.sharedSecret(hs.bytes("deviceEphPriv"), ByteArray(32)) }
        val signature = hs.bytes("hs2Sig").apply { this[0] = (this[0].toInt() xor 1).toByte() }
        assertFalse(RelayCrypto.verify(keys.bytes("hostSignPub"), hs.bytes("transcriptHash"), signature))
    }

    @Test fun `reassembly preserves message boundaries and rejects replay`() {
        val key = hs.bytes("kD2H")
        val message = ByteArray(RelayCrypto.CHUNK_SIZE + 17) { (it % 251).toByte() }
        val sealer = FrameSealer(key)
        val opener = FrameOpener(key)
        val frames = sealer.seal(message)
        assertEquals(2, frames.size)
        assertEquals(null, opener.open(frames[0]))
        assertArrayEquals(message, opener.open(frames[1]))
        val second = sealer.seal(byteArrayOf(3)).single()
        assertArrayEquals(byteArrayOf(3), opener.open(second))
        assertFails<IllegalArgumentException> { opener.open(second) }
    }

    @Test fun `bad tags oversize messages invalid flags and skipped counters are rejected`() {
        val key = hs.bytes("kD2H")
        val frame = FrameSealer(key).seal(byteArrayOf(1, 2, 3)).single()
        assertFails<IllegalArgumentException> { FrameOpener(key).open(frame.copy(counter = 1)) }
        val corrupt = frame.data.copyOf().apply { this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte() }
        assertFails<SecurityException> { FrameOpener(key).open(frame.copy(data = corrupt)) }
        assertFails<IllegalArgumentException> { FrameOpener(key, limit = 2).open(frame) }
        assertFails<IllegalArgumentException> { FrameSealer(key).seal(ByteArray(RelayCrypto.MAX_OUTBOUND + 1)) }
        val malformed = RelayCrypto.crypt(true, key, ByteArray(12), RelayCrypto.label("codync/frame/v1") + 0L.bigEndian(), byteArrayOf(2))
        assertFails<IllegalArgumentException> { RelayCrypto.openFrame(key, 0, malformed) }
    }
}

internal inline fun <reified T : Throwable> assertFails(block: () -> Unit): T {
    try { block() } catch (error: Throwable) {
        if (error is T) return error
        throw AssertionError("Expected ${T::class.simpleName}, got ${error::class.simpleName}", error)
    }
    throw AssertionError("Expected ${T::class.simpleName}")
}
