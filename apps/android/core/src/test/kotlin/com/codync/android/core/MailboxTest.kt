package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class MailboxTest {
    private class Relay : ChannelSocket {
        val incoming = Channel<String>(16)
        val blobs = mutableListOf<String>()
        var enqueueError: String? = null
        init { incoming.trySend("""{"t":"presence","online":false}""") }
        override suspend fun receive(): String = incoming.receive()
        override suspend fun send(text: String) {
            val message = WireJson.parseToJsonElement(text).jsonObject
            val response = when (message.text("t")) {
                "mbox.put" -> { blobs.add(message.getValue("d").jsonPrimitive.content)
                    buildJsonObject { put("t", if (enqueueError == null) "mbox.ok" else "mbox.err"); put("nonce", message.getValue("nonce")); enqueueError?.let { put("code", it) } } }
                "mbox.list" -> buildJsonObject { put("t", "mbox.items"); put("items", JsonArray(emptyList())) }
                "mbox.cancel" -> buildJsonObject { put("t", "mbox.gone"); put("nonce", message.getValue("nonce")); put("state", "delivering") }
                else -> return
            }
            incoming.send(response.toString())
        }
        override fun close(code: Int) { incoming.close() }
    }
    private suspend fun connect(relay: Relay): EncryptedChannel {
        val key = RelayCrypto.signingPublic(ByteArray(32) { 1 })
        val computer = Computer(RelayCrypto.computerId(key), "Computer", key.base64Url(), RelayCrypto.agreementPublic(ByteArray(32) { 2 }).base64Url())
        val channel = EncryptedChannel(relay, HostRoute.Relay, computer, DeviceIdentity(ByteArray(32) { 3 }, ByteArray(32) { 4 }), false)
        return channel.start()
    }
    @Test fun `offline mailbox retries use fresh seals without claiming a cancellation succeeded`() = runBlocking {
        val relay = Relay()
        val channel = connect(relay)
        try {
            assertTrue(channel.state.value is LinkState.ComputerOffline)
            val send = PendingSend("stable", "bot", text = "message", createdAt = 1)
            withTimeout(3_000) { channel.enqueue(send); channel.enqueue(send) }
            assertEquals(2, relay.blobs.size)
            assertNotEquals(relay.blobs[0], relay.blobs[1])
            val queued = channel.queued()
            assertTrue(queued.getValue("items").jsonArray.isEmpty())
            assertEquals("delivering", channel.cancelQueued(send.nonce))
        } finally { channel.close() }
    }
    @Test fun `relay rejections remain explicit for host online races and limits`() = runBlocking {
        val relay = Relay().apply { enqueueError = "hostOnline" }
        val channel = connect(relay)
        try {
            try { channel.enqueue(PendingSend("stable", "bot", text = "message", createdAt = 1)); fail("Expected hostOnline") }
            catch (failure: IOException) { assertEquals("hostOnline", failure.message) }
        } finally { channel.close() }
    }
}
