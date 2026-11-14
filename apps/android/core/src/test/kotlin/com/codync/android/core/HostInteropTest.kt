package com.codync.android.core

import java.io.File
import java.security.SecureRandom
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertArrayEquals
import kotlinx.serialization.json.*
import java.util.UUID
import kotlinx.coroutines.delay
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Opt-in live acceptance. Inputs and the cleanup public key stay in private temporary files. */
class HostInteropTest {
    @Test fun `pair hello and sync work against a real host on the requested route`() = runBlocking<Unit> {
        val path = System.getenv("CODYNC_ANDROID_PAIRING_FILE")
        assumeTrue("No live host pairing was requested", path != null)
        // Real hosts emit their own environment's scheme (codync-dev:// from a development build).
        val link = File(requireNotNull(path)).readText()
        val parsed = Pairing.parse(link, link.trim().substringBefore("://"))
        val relay = System.getenv("CODYNC_ANDROID_TEST_ROUTE") == "relay"
        val pairing = if (relay) parsed.copy(computer = parsed.computer.copy(route = ConnectionRoute.CloudflareFirst, urls = emptyList())) else parsed
        val random = SecureRandom()
        // Pairing needs the account's word for the device key: a cloud certificate and the seed it was issued for.
        val proof = Json.parseToJsonElement(File(requireNotNull(System.getenv("CODYNC_ANDROID_CERTIFICATE_FILE")) {
            "Live pairing requires a certificate file: {seed, payload, sig}" }).readText()).jsonObject
        val identity = DeviceIdentity(requireNotNull(proof.text("seed")).decodeBase64Url(32), ByteArray(32).also(random::nextBytes))
        val certificate = DeviceCertificate(requireNotNull(proof.text("payload")), requireNotNull(proof.text("sig")))
        val cleanup = System.getenv("CODYNC_ANDROID_CLEANUP_KEY_FILE")
        requireNotNull(cleanup) { "Live acceptance requires a cleanup key file" }
        File(cleanup).writeText(identity.publicKey)
        val paired = withTimeout(150_000) { HostConnector.pair(pairing, identity, "Android protocol acceptance", certificate).computer }
        val target = paired.copy(route = if (relay) ConnectionRoute.CloudflareFirst else ConnectionRoute.DirectOnly,
            urls = if (relay) emptyList() else paired.urls)
        val channel = withTimeout(30_000) { HostConnector.connect(target, identity) }
        try {
            assertEquals(if (relay) HostRoute.Relay else HostRoute.Direct, channel.route)
            val hello = HostClient.hello(channel, target)
            assertEquals(pairing.computer.id, hello.id)
            HostClient.bots(channel)
            System.getenv("CODYNC_ANDROID_FILE_TEST_BOT")?.let { bot ->
                val root = kotlin.io.path.createTempDirectory("codync-live-files").toFile()
                try {
                    val bytes = ByteArray(HostFiles.CHUNK_BYTES + 13) { (it % 255).toByte() }
                    val source = root.resolve("acceptance.bin").apply { writeBytes(bytes) }
                    val upload = UUID.randomUUID().toString()
                    HostFiles(channel).upload(bot, upload, OutgoingFile(source.path, source.name, source.length()))
                    val nonce = UUID.randomUUID().toString()
                    val body = buildJsonObject { put("botId", bot); put("text", ""); put("clientNonce", nonce); put("attachments", JsonArray(listOf(JsonPrimitive(upload)))) }
                    val result = channel.call("send", body)
                    val entry = Entry.decode(result.jsonObject.getValue("entry").jsonObject)
                    val attachment = WireJson.decodeFromJsonElement<Attachment>(entry.data.getValue("attachments").jsonArray.single())
                    assertEquals(source.length(), attachment.size)
                    val duplicate = channel.call("send", body)
                    assertEquals(entry.id, duplicate.jsonObject.getValue("entry").jsonObject.getValue("id").jsonPrimitive.content)
                    val saved = root.resolve("download")
                    HostFiles(channel).download(bot, attachment, saved)
                    assertArrayEquals(bytes, saved.readBytes())
                    withTimeout(20_000) {
                        var answered = false
                        while (true) {
                            val history = channel.call("history", buildJsonObject { put("botId", bot); put("limit", 100) })
                            val entries = history.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
                            val card = entries.firstOrNull { it.kind == "permission" && it.status == "pending" }
                            if (card != null && !answered) {
                                val option = card.data.getValue("options").jsonArray.first().jsonObject.getValue("optionId").jsonPrimitive.content
                                channel.call("respondPermission", buildJsonObject { put("entryId", card.id); put("optionId", option) })
                                answered = true
                            }
                            if (answered && entries.any { it.seq > entry.seq && it.kind == "agent" && it.isChat && it.text.startsWith("Done:") }) break
                            delay(100)
                        }
                    }
                } finally { root.deleteRecursively() }
            }
        } finally { channel.close() }
    }
}
