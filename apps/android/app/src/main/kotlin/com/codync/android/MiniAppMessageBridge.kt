package com.codync.android

import android.webkit.WebView
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import com.codync.android.core.MINI_APP_ORIGIN
import com.codync.android.core.MiniAppAddress
import com.codync.android.core.MiniAppAnswer
import com.codync.android.core.MiniAppPageGate
import com.codync.android.core.WireJson
import java.util.Base64
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Serializable private data class MiniAppMessage(val id: Int, val method: String, val path: String, val body: String = "")

/** Only Android's authenticated main-frame messages may reach this app's host capability. */
internal class MiniAppMessageBridge(
    private val address: MiniAppAddress,
    private val store: AppStore,
    private val scope: CoroutineScope,
    private val onFailure: (String) -> Unit,
) {
    private val page = MiniAppPageGate(address)

    fun pageStarted(url: String?) = page.started(url)

    fun install(view: WebView): Boolean {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            onFailure("Update Android System WebView to open mini-apps securely.")
            return false
        }
        WebViewCompat.addWebMessageListener(view, "CodyncBridge", setOf(MINI_APP_ORIGIN)) { source, message, origin, mainFrame, reply ->
            val ticket = page.admit(origin.toString(), mainFrame, source.url) ?: return@addWebMessageListener
            if (message.type != WebMessageCompat.TYPE_STRING) return@addWebMessageListener
            val request = try {
                WireJson.decodeFromString<MiniAppMessage>(message.data ?: return@addWebMessageListener)
            } catch (error: Exception) {
                onFailure("The mini-app sent an invalid request.")
                return@addWebMessageListener
            }
            scope.launch {
                if (!page.mayReply(ticket, source.url)) return@launch
                val answer = relay(request)
                if (!page.mayReply(ticket, source.url)) return@launch
                // The proxy addresses the original frame, never a replacement page at the same URL.
                reply.postMessage(buildJsonObject {
                    put("id", request.id); put("status", answer.status)
                    put("type", answer.contentType); put("body", answer.body)
                }.toString())
            }
        }
        return true
    }

    private suspend fun relay(request: MiniAppMessage): MiniAppAnswer = try {
        val relative = address.relativePath(request.path.substringBefore('?')) ?: error("Outside this mini-app")
        val query = request.path.substringAfter('?', "")
        store.appRequest(address, request.method, if (query.isEmpty()) relative else "$relative?$query", request.body)
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        MiniAppAnswer(502, "application/json", "", Base64.getEncoder().encodeToString(
            """{"error":${JsonPrimitive(error.message ?: "Could not reach the computer.")}}""".toByteArray()))
    }
}
