package com.codync.android

import android.annotation.SuppressLint
import android.content.Context
import android.view.View
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Artifact `id`'s HTML in a web view of its own: scripts on, no bridge into the app, no files, and an origin of its own,
 * so what it keeps (a tracker's ticks) stays its own. In the viewer (`interactive`) a link the user taps opens in the
 * system browser; the card's preview follows none and is hidden from TalkBack. `gone`: the artifact crashed its renderer,
 * so this view can't be used again and the caller drops it.
 */
@Composable internal fun ArtifactWebView(id: String, html: String, interactive: Boolean, gone: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val links = LocalUriHandler.current
    val onGone by rememberUpdatedState(gone)
    val view = remember(id) { ArtifactView(context, id, interactive, { url -> runCatching { links.openUri(url) } }) { onGone() } }
    AndroidView({ view }, modifier, onRelease = ArtifactView::destroy, update = { it.show(html) })
}

@SuppressLint("SetJavaScriptEnabled", "ViewConstructor")
private class ArtifactView(context: Context, id: String, interactive: Boolean, open: (String) -> Unit, gone: () -> Unit) : WebView(context) {
    private var shown: String? = null
    /** Never resolves: relative links go nowhere, and the artifact gets an origin to keep its storage under (as on iPhone). */
    private val origin = "${id.lowercase()}.artifact.codync.invalid"

    init {
        settings.javaScriptEnabled = true
        settings.domStorageEnabled = true
        settings.allowFileAccess = false
        settings.allowContentAccess = false
        settings.javaScriptCanOpenWindowsAutomatically = false
        settings.setSupportMultipleWindows(false)
        isVerticalScrollBarEnabled = false
        isHorizontalScrollBarEnabled = false
        if (!interactive) importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val url = request.url
                // In-artifact anchors stay; a tapped link leaves for the browser, an artifact sending itself elsewhere goes nowhere.
                if (url.scheme == "about" || url.host == origin) return false
                if (interactive && request.hasGesture() && url.scheme in LINK_SCHEMES) open(url.toString())
                return true
            }

            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                gone()
                return true
            }
        }
    }

    /** Loads `html` unless it's what's shown already (the same version keeps its scroll and state). */
    fun show(html: String) {
        if (shown === html) return
        shown = html
        loadDataWithBaseURL("https://$origin/", html, "text/html", "UTF-8", null)
    }
}

private val LINK_SCHEMES = listOf("http", "https", "mailto", "tel")
