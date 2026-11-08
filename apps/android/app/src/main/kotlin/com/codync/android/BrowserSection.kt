package com.codync.android

import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * The computer's browser: its tabs, and a picture of the page it shows. Typing and clicking happen in the
 * browser's own window; this screen opens, switches, reloads and closes its tabs.
 */
@Composable internal fun BrowserSection(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var tabs by remember { mutableStateOf<List<Pair<String, String>>>(emptyList()) }
    var active by remember { mutableStateOf<String?>(null) }
    var image by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
    var address by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun adopt(json: JsonObject) {
        tabs = json.browserTabs()
        active = json["activeId"]?.jsonPrimitive?.content ?: tabs.firstOrNull()?.first
        error = null
    }
    fun run(block: suspend () -> Unit) = scope.launch {
        busy = true
        try { block() } catch (e: Exception) { error = e.message }
        busy = false
    }
    fun shot() = scope.launch {
        val tab = active ?: return@launch
        try {
            val data = store.browserShot(tab) ?: return@launch
            val bytes = Base64.decode(data, Base64.DEFAULT)
            image = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
        } catch (e: Exception) { error = e.message }
    }
    LaunchedEffect(state.contextId) { run { adopt(store.browserPanel()) } }
    LaunchedEffect(active) { shot() }
    CardSection("Browser", "The computer's own browser. Type and click in its window; this screen shows the page and keeps its tabs.") {
        row {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (tabs.isEmpty()) Text("No tabs yet.", style = Type.caption, color = Palette.secondary)
                LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    items(tabs, key = { it.first }) { (id, title) ->
                        TabChip(title, id == active,
                            close = if (tabs.size > 1) ({ run { adopt(store.browserClose(id)) } }) else null,
                            select = { run { adopt(store.browserActivate(id)) } })
                    }
                }
                FieldBox(address, { address = it }, "Address", 1, 1, fill = Palette.bubbleUser, capitalize = false)
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    SecondaryButton(if (active == null) "Open" else "Go", enabled = !busy && address.isNotBlank()) {
                        val url = address.trim()
                        run {
                            adopt(if (active == null) store.browserOpen(url) else store.browserNavigate(active!!, url))
                            address = ""
                        }
                    }
                    IconButton("New tab", "plus") {
                        run {
                            adopt(store.browserOpen(address.trim().ifEmpty { "about:blank" }))
                            address = ""
                        }
                    }
                    IconButton("Reload", "arrow.clockwise") { shot() }
                    IconButton("Close the browser", "xmark") {
                        run { adopt(store.browserShutdown()); image = null }
                    }
                }
                image?.let {
                    Image(it.asImageBitmap(), "Browser page", Modifier.fillMaxWidth().padding(top = 4.dp),
                        contentScale = ContentScale.FillWidth)
                }
                error?.let { Text(it, style = Type.caption, color = Palette.danger) }
            }
        }
    }
}

@Composable private fun TabChip(title: String, selected: Boolean, close: (() -> Unit)?, select: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier
            .then(if (selected) Modifier.background(Palette.bubbleUser, RoundedCornerShape(8.dp)) else Modifier)
            .padding(horizontal = 8.dp, vertical = 4.dp)) {
        Text(title, style = Type.caption, color = if (selected) Palette.text else Palette.secondary,
            modifier = Modifier.clickable { select() })
        close?.let { IconButton("Close tab", "xmark", action = it) }
    }
}
