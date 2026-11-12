package com.codync.android.core

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.channels.Channel
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString

internal interface ChannelSocket {
    suspend fun send(text: String)
    suspend fun receive(): String
    fun close(code: Int = 1000)
}

class ChannelClosed(val code: Int) : IOException(when (code) {
    4001, 4003 -> "This device isn't allowed on the computer. Pair it again."
    4400 -> "Update Codync to connect to this computer."
    4410 -> "This pairing code expired. Show a new one on the computer."
    else -> "Connection to the computer ended."
})

/** The upgrade was answered with plain HTTP; `appFloor` is the cloud's reason when it asks this app to update. */
internal class SocketRefused(val status: Int, val appFloor: String? = null) : IOException("The connection was refused ($status).")

/** `{"error":{"code":"appUpdateRequired","message"}}`: the cloud's emergency floor for this app version. */
private fun appFloor(response: Response): String? = runCatching {
    val error = Json.parseToJsonElement(response.peekBody(16 * 1024).string()).jsonObject["error"]?.jsonObject
    error?.text("message")?.takeIf { error.text("code") == "appUpdateRequired" }
}.getOrNull()

internal class OkHttpChannelSocket private constructor(client: OkHttpClient, request: Request) : ChannelSocket {
    private val opened = CompletableDeferred<Unit>()
    private val incoming = Channel<String>(16)
    private val socket = client.newWebSocket(request, object : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) { opened.complete(Unit) }
        override fun onMessage(webSocket: WebSocket, text: String) {
            if (text.toByteArray().size > 1024 * 1024) {
                incoming.close(IOException("The computer sent an oversized channel message."))
                webSocket.close(1009, null)
                return
            }
            // This callback runs on OkHttp's socket reader, never the UI thread.
            // Waiting here propagates bounded backpressure to the TCP reader.
            try { runBlocking { incoming.send(text) } }
            catch (_: Exception) { webSocket.cancel() }
        }
        override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
            incoming.close(IOException("Unexpected binary channel message"))
            webSocket.close(1003, null)
        }
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            incoming.close(ChannelClosed(code))
            opened.completeExceptionally(ChannelClosed(code))
            webSocket.close(code, null)
        }
        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            incoming.close(ChannelClosed(code))
            opened.completeExceptionally(ChannelClosed(code))
        }
        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            val failure = response?.let { SocketRefused(it.code, appFloor(it)) } ?: IOException("Can't reach the computer.", t)
            opened.completeExceptionally(failure)
            incoming.close(failure)
        }
    })

    override suspend fun send(text: String) {
        opened.await()
        if (!socket.send(text)) throw IOException("Connection to the computer ended.")
    }
    override suspend fun receive(): String = incoming.receive()
    override fun close(code: Int) {
        socket.cancel()
        incoming.cancel(CancellationException("Socket closed ($code)"))
        opened.cancel()
    }

    companion object {
        // Native pings detect dead direct links even when no application events arrive.
        private val client = OkHttpClient.Builder().pingInterval(15, TimeUnit.SECONDS)
            .connectTimeout(10, TimeUnit.SECONDS).readTimeout(0, TimeUnit.SECONDS).build()
        suspend fun dial(request: Request): ChannelSocket {
            val result = OkHttpChannelSocket(client, request)
            try {
                result.opened.await()
                return result
            } catch (error: Throwable) {
                result.close()
                throw error
            }
        }
    }
}
