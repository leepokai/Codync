package com.codync.android

import android.content.ActivityNotFoundException
import android.content.Intent
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.core.CallSearchResult
import com.codync.android.design.*

private const val SEARCH_POLICY = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"

/** Static provider HTML: no script, filesystem, network requests, cache, cookies, or DOM storage. */
@Composable internal fun CallSearchView(result: CallSearchResult) {
    val context = LocalContext.current
    var linkError by remember(result.html) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().background(Palette.background, RoundedCornerShape(16.dp)).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr("Google Search"), style = Type.caption.semibold, color = Palette.text)
        if (result.text.isNotEmpty()) Text(result.text, Modifier.heightIn(max = 100.dp).verticalScroll(rememberScrollState()),
            style = Type.footnote, color = Palette.text)
        AndroidView(modifier = Modifier.fillMaxWidth().height(120.dp).clip(RoundedCornerShape(8.dp)), factory = {
            WebView(it).apply {
                settings.javaScriptEnabled = false
                settings.domStorageEnabled = false
                settings.allowFileAccess = false
                settings.allowContentAccess = false
                settings.blockNetworkLoads = true
                settings.cacheMode = WebSettings.LOAD_NO_CACHE
                settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
                webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                        if (request.hasGesture() && request.isForMainFrame && request.url.scheme?.lowercase() in listOf("http", "https")) {
                            try { context.startActivity(Intent(Intent.ACTION_VIEW, request.url)) }
                            catch (_: ActivityNotFoundException) { linkError = true }
                        }
                        return true
                    }
                }
            }
        }, update = {
            if (it.tag != result.html) {
                it.tag = result.html
                it.loadDataWithBaseURL(null, SEARCH_POLICY + result.html, "text/html", "UTF-8", null)
            }
        }, onRelease = { it.stopLoading(); it.destroy() })
        if (linkError) Text(tr("Couldn't open the link."), style = Type.caption, color = Palette.danger)
    }
}
