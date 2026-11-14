package com.codync.android

import android.annotation.SuppressLint
import android.net.Uri
import android.view.accessibility.AccessibilityManager
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import com.codync.android.core.TerminalSink
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import java.io.ByteArrayInputStream
import java.util.Base64
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CompletableDeferred
import com.codync.android.core.deadline

/** Only bundled terminal assets run here. Terminal output is bytes; it never becomes HTML or JavaScript. */
internal class TerminalRenderer internal constructor(internal val view: WebView) : TerminalSink {
    private var nextWrite = 0
    private val writes = mutableMapOf<Int, CompletableDeferred<Unit>>()
    var disposed = false
        private set
    override suspend fun write(bytes: ByteArray) {
        check(!disposed && bytes.size <= 256 * 1024)
        val id = ++nextWrite
        val done = CompletableDeferred<Unit>()
        writes[id] = done
        val encoded = Base64.getEncoder().encodeToString(bytes)
        view.evaluateJavascript("codyncTerminal.write($id, '$encoded')", null)
        try { deadline(10_000, "The terminal stopped responding.") { done.await() } } finally { writes.remove(id) }
    }
    override fun reset() { if (!disposed) view.evaluateJavascript("codyncTerminal.reset()", null) }
    override fun input(enabled: Boolean) { if (!disposed) view.evaluateJavascript("codyncTerminal.input($enabled)", null) }
    fun focus(showKeyboard: Boolean = false) {
        if (disposed) return
        view.requestFocus()
        view.evaluateJavascript("codyncTerminal.focus()") {
            if (showKeyboard && !disposed) view.context.getSystemService(android.view.inputmethod.InputMethodManager::class.java)
                .showSoftInput(view, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT)
        }
    }
    fun copySelection() { if (!disposed) view.evaluateJavascript("codyncTerminal.copySelection()", null) }
    fun screenReader(enabled: Boolean) {
        if (!disposed) view.evaluateJavascript("window.codyncTerminal?.screenReader($enabled)", null)
    }
    fun written(id: Int) { writes.remove(id)?.complete(Unit) }
    fun dispose() { disposed = true; writes.values.forEach { it.cancel() }; writes.clear() }
}

/** A terminal edits a remote command line; the IME must not rewrite earlier commands. */
private class TerminalWebView(context: android.content.Context) : WebView(context) {
    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        val connection = super.onCreateInputConnection(info)
        if (connection != null && info.inputType and android.text.InputType.TYPE_MASK_CLASS == android.text.InputType.TYPE_CLASS_TEXT) {
            info.inputType = info.inputType or android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            info.imeOptions = info.imeOptions or EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING or EditorInfo.IME_FLAG_NO_EXTRACT_UI
        }
        return connection
    }
}

@SuppressLint("SetJavaScriptEnabled")
@Composable internal fun TerminalSurface(modifier: Modifier, ready: (TerminalRenderer) -> Unit,
    input: (ByteArray) -> Unit, resized: (Int, Int) -> Unit, failure: () -> Unit,
    openLink: (Uri) -> Unit = {}, copy: (String) -> Unit = {}, created: (TerminalRenderer) -> Unit = {}) {
    val context = LocalContext.current
    val accessibility = context.getSystemService(AccessibilityManager::class.java)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val onReady by rememberUpdatedState(ready); val onInput by rememberUpdatedState(input)
    val onResize by rememberUpdatedState(resized); val onFailure by rememberUpdatedState(failure)
    val onOpen by rememberUpdatedState(openLink); val onCopy by rememberUpdatedState(copy)
    val view = remember {
        TerminalWebView(context).apply {
            layoutParams = android.view.ViewGroup.LayoutParams(android.view.ViewGroup.LayoutParams.MATCH_PARENT, android.view.ViewGroup.LayoutParams.MATCH_PARENT)
            settings.javaScriptEnabled = true
            settings.useWideViewPort = true
            settings.allowFileAccess = false; settings.allowContentAccess = false
            settings.blockNetworkLoads = true; settings.domStorageEnabled = false
            settings.javaScriptCanOpenWindowsAutomatically = false
            settings.setSupportMultipleWindows(false)
            settings.cacheMode = android.webkit.WebSettings.LOAD_NO_CACHE
            setBackgroundColor(android.graphics.Color.rgb(17, 17, 17))
            importantForAutofill = android.view.View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS
            isFocusableInTouchMode = true
        }
    }
    DisposableEffect(view, lifecycle) {
        val closed = AtomicBoolean(false)
        var destroyed = false
        // Xterm's accessibility path intentionally suppresses ordinary insertText events.
        // Use it for a screen reader rather than suppressing Android IME commits for everyone.
        val screenReader = AtomicBoolean(accessibility.isTouchExplorationEnabled)
        val renderer = TerminalRenderer(view)
        created(renderer)
        fun main(block: () -> Unit) { view.post { if (!closed.get()) block() } }
        val bridge = object {
            @JavascriptInterface fun screenReaderEnabled(): Boolean = screenReader.get()
            @JavascriptInterface fun ready() = main { renderer.focus(); onReady(renderer) }
            @JavascriptInterface fun failed() = main { onFailure() }
            @JavascriptInterface fun resized(cols: Int, rows: Int) = main { if (cols in 20..500 && rows in 4..500) onResize(cols, rows) }
            @JavascriptInterface fun written(id: Int) = main { renderer.written(id) }
            @JavascriptInterface fun input(encoded: String) {
                if (encoded.length > 90_000) { main { renderer.input(false); onFailure() }; return }
                val bytes = try { Base64.getDecoder().decode(encoded) } catch (_: Exception) { main { onFailure() }; return }
                main { onInput(bytes) }
            }
            @JavascriptInterface fun openLink(text: String) = main {
                if (text.length > 8_192 || text.any { it.isISOControl() }) return@main
                val uri = Uri.parse(text)
                if (uri.scheme in listOf("http", "https") && !uri.host.isNullOrBlank() && uri.userInfo == null) onOpen(uri)
            }
            @JavascriptInterface fun copy(text: String) = main { if (text.isNotEmpty() && text.length <= 256 * 1024) onCopy(text) }
        }
        view.addJavascriptInterface(bridge, "CodyncTerminal")
        val accessibilityListener = AccessibilityManager.TouchExplorationStateChangeListener { enabled ->
            screenReader.set(enabled)
            main { renderer.screenReader(enabled) }
        }
        accessibility.addTouchExplorationStateChangeListener(accessibilityListener)
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) view.onResume()
            if (event == Lifecycle.Event.ON_PAUSE) view.onPause()
        }
        lifecycle.addObserver(observer)
        view.webViewClient = object : WebViewClient() {
            override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                if (!closed.getAndSet(true)) {
                    renderer.dispose()
                    (view.parent as? android.view.ViewGroup)?.removeView(view)
                    view.destroy(); destroyed = true
                    onFailure()
                }
                return true
            }
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = true
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                val uri = request.url
                val name = uri.path?.removePrefix("/")
                val files = mapOf("index.html" to "text/html", "xterm.js" to "text/javascript", "addon-fit.js" to "text/javascript", "addon-web-links.js" to "text/javascript",
                    "bridge.js" to "text/javascript", "guard.js" to "text/javascript", "xterm.css" to "text/css", "terminal.css" to "text/css")
                if (uri.scheme == "https" && uri.host == "terminal.codync.invalid" && uri.port == -1 && uri.query == null && name in files)
                    return WebResourceResponse(files.getValue(requireNotNull(name)), "UTF-8", context.assets.open("terminal/$name"))
                return WebResourceResponse("text/plain", "UTF-8", 403, "Blocked", emptyMap(), ByteArrayInputStream(byteArrayOf()))
            }
        }
        // Loading before AndroidView's first layout gives WebView a zero-height CSS viewport.
        var loaded = false
        val layout = android.view.View.OnLayoutChangeListener { _, left, top, right, bottom, _, _, _, _ ->
            if (!loaded && right > left && bottom > top) {
                loaded = true
                view.loadUrl("https://terminal.codync.invalid/index.html")
            }
        }
        view.addOnLayoutChangeListener(layout)
        onDispose {
            accessibility.removeTouchExplorationStateChangeListener(accessibilityListener)
            view.removeOnLayoutChangeListener(layout)
            lifecycle.removeObserver(observer)
            closed.set(true); renderer.dispose()
            if (!destroyed) {
                view.stopLoading(); view.removeJavascriptInterface("CodyncTerminal")
                view.loadUrl("about:blank"); view.destroy(); destroyed = true
            }
        }
    }
    AndroidView(factory = { view }, modifier = modifier)
}
