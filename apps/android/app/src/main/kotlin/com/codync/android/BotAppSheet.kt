package com.codync.android

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.webkit.WebView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.core.BotApp
import com.codync.android.core.MiniAppAccess
import com.codync.android.core.MiniAppAddress
import com.codync.android.core.miniAppAccess
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** The owner's preview and separately granted share links never use the same capability. */
@SuppressLint("SetJavaScriptEnabled")
@Composable internal fun BotAppSheet(botId: String, app: BotApp, store: AppStore, onChanged: () -> Unit = {}) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var shareMode by remember { mutableStateOf("view") }
    var sharing by remember { mutableStateOf(false) }
    var shareStatus by remember { mutableStateOf<String?>(null) }
    var shareError by remember { mutableStateOf<String?>(null) }
    var openingError by remember(app.url) { mutableStateOf<String?>(null) }
    var retry by remember { mutableStateOf(0) }
    Column(Modifier.fillMaxSize()) {
        ModalHeader("${app.title} · v${app.version}") {
            ChoicePicker(shareMode, listOf("view" to "View", "edit" to "Edit"), { shareMode = it }, label = "Share mode", enabled = !sharing)
            IconButton("Share link", "square.and.arrow.up", enabled = !sharing) {
                sharing = true
                shareError = null
                shareStatus = null
                scope.launch {
                    try {
                        val result = store.createAppLink(botId, app.slug, shareMode, "Mobile $shareMode")
                        val url = result.shareUrl()
                        val clipboard = context.getSystemService(ClipboardManager::class.java)
                            ?: error("Clipboard is unavailable.")
                        clipboard.setPrimaryClip(ClipData.newPlainText("Mini-App Link", url))
                        shareStatus = if (miniAppAccess(url) == MiniAppAccess.ComputerOnly)
                            "Link copied. It opens only on the host computer."
                        else "${shareMode.replaceFirstChar(Char::uppercase)} link copied."
                        onChanged()
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (error: Exception) {
                        shareError = error.message ?: "Could not create the share link."
                    } finally {
                        sharing = false
                    }
                }
            }
        }
        shareStatus?.let { Text(it, Modifier.padding(horizontal = 20.dp, vertical = 6.dp), style = Type.caption, color = Palette.accent) }
        shareError?.let { Text(it, Modifier.padding(horizontal = 20.dp, vertical = 6.dp), style = Type.footnote, color = Palette.danger) }
        val address = MiniAppAddress.parse(app.url)
        if (address == null) {
            Box(Modifier.fillMaxWidth().weight(1f).padding(20.dp), contentAlignment = Alignment.Center) {
                Text("The computer has not provided a usable address for this mini-app. Open it in Codync on that computer.",
                    style = Type.body, color = Palette.secondary)
            }
        } else {
            openingError?.let { message ->
                Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(message, style = Type.footnote, color = Palette.danger)
                    SecondaryButton("Retry opening") { openingError = null; retry += 1 }
                }
            }
            key(app.url, retry) {
                AndroidView(modifier = Modifier.fillMaxWidth().weight(1f), factory = { ctx ->
                    WebView(ctx).apply {
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = false
                        settings.allowFileAccess = false
                        settings.allowContentAccess = false
                        val bridge = MiniAppMessageBridge(address, store, scope) { openingError = it }
                        webViewClient = MiniAppWebClient(address, store, bridge::pageStarted) { openingError = it }
                        if (bridge.install(this)) loadUrl(address.pageUrl)
                    }
                }, onReset = null, onRelease = { it.destroy() })
            }
        }
    }
}
