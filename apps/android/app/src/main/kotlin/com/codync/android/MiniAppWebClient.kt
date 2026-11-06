package com.codync.android

import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import com.codync.android.core.MiniAppNavigation
import com.codync.android.core.miniAppHasVirtualOrigin
import com.codync.android.core.miniAppNavigation
import com.codync.android.core.MiniAppAddress
import com.codync.android.core.miniAppPolicy
import java.io.ByteArrayInputStream
import java.util.Base64
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

private const val BRIDGE_PATH = "/_codync_bridge.js"
private val headTag = Regex("<head[^>]*>", RegexOption.IGNORE_CASE)

/** Serves a mini-app to a phone WebView by asking the host for every request (pages, files, data, tells). */
internal class MiniAppWebClient(
    private val address: MiniAppAddress,
    private val store: AppStore,
    private val pageStarted: (String?) -> Unit,
    private val onFailure: (String) -> Unit,
) : WebViewClient() {
    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
        when (miniAppNavigation(address, request.url.toString())) {
            MiniAppNavigation.InApp -> false
            MiniAppNavigation.Browser -> { openBrowser(view, request.url); true }
            MiniAppNavigation.Blocked -> { onFailure("This mini-app cannot open that address."); true }
        }

    override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) {
        pageStarted(url)
        if (url != null && miniAppNavigation(address, url) != MiniAppNavigation.InApp) {
            // POST navigations do not call shouldOverrideUrlLoading. They cannot retain the bridge either.
            view.stopLoading()
            if (miniAppNavigation(address, url) == MiniAppNavigation.Browser) openBrowser(view, Uri.parse(url))
            else onFailure("This mini-app cannot open that address.")
        }
    }

    private fun openBrowser(view: WebView, url: Uri) {
        try {
            view.context.startActivity(Intent(Intent.ACTION_VIEW, url).addCategory(Intent.CATEGORY_BROWSABLE)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (error: Exception) {
            onFailure("No browser is available to open this link.")
        }
    }
    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url
        if (!miniAppHasVirtualOrigin(url.toString())) {
            return if (request.isForMainFrame) response(403, "text/plain", "Open external pages in your browser".toByteArray(), "") else null
        }
        val path = url.path.orEmpty()
        if (path == BRIDGE_PATH && !request.isForMainFrame) {
            return response(200, "text/javascript", miniAppBridgeScript().toByteArray(), "")
        }
        val relative = address.relativePath(path) ?: return response(403, "text/plain", "Outside this mini-app".toByteArray(), "")
        if (request.method != "GET") return response(405, "text/plain", ByteArray(0), "")
        val target = url.encodedQuery?.let { "$relative?$it" } ?: relative
        return try {
            val answer = runBlocking { store.appRequest(address, "GET", target) }
            val bytes = Base64.getDecoder().decode(answer.body)
            val isPage = request.isForMainFrame && answer.contentType.startsWith("text/html")
            response(answer.status, answer.contentType, if (isPage) withBridge(bytes) else bytes, answer.csp)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            if (request.isForMainFrame) view.post { onFailure(error.message ?: "Could not reach the computer.") }
            response(502, "text/plain", ByteArray(0), "")
        }
    }

    private fun withBridge(html: ByteArray): ByteArray {
        val text = String(html, Charsets.UTF_8)
        val tag = "<script src=\"$BRIDGE_PATH\"></script>"
        val head = headTag.find(text)
        return (if (head != null) text.replaceRange(head.range.last + 1, head.range.last + 1, tag) else tag + text).toByteArray()
    }

    private fun response(status: Int, contentType: String, body: ByteArray, csp: String): WebResourceResponse {
        val mime = contentType.substringBefore(';').trim().ifEmpty { "application/octet-stream" }
        val headers = mutableMapOf("Cache-Control" to "no-store")
        miniAppPolicy(csp).takeIf { it.isNotEmpty() }?.let { headers["Content-Security-Policy"] = it }
        return WebResourceResponse(mime, "utf-8", status, if (status < 400) "OK" else "Error", headers, ByteArrayInputStream(body))
    }
}
