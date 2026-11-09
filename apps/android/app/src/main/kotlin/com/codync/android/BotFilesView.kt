package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private fun sizeText(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> "${bytes / 1024} KB"
    else -> String.format("%.1f MB", bytes / 1024.0 / 1024.0)
}

/** One folder of a bot's workspace at a time; folders open, files copy their path. The host confines every path. */
@Composable internal fun BotFilesView(bot: Bot, store: AppStore) {
    val context = LocalContext.current
    var path by remember(bot.id) { mutableStateOf("") }
    var rows by remember(bot.id) { mutableStateOf<JsonArray?>(null) }
    var error by remember(bot.id) { mutableStateOf<String?>(null) }
    LaunchedEffect(bot.id, path) {
        try {
            rows = store.listFiles(bot.id, path)
            error = null
        } catch (e: Exception) {
            error = e.message
        }
    }
    val segments = path.split('/').filter { it.isNotBlank() }
    fun copyPath(name: String) {
        val full = listOf(bot.cwd.trimEnd('/', '\\')) + segments + name
        val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
        clipboard?.setPrimaryClip(ClipData.newPlainText("path", full.joinToString("/")))
    }
    CardForm {
        CardSection("Files", footer = "Inside this bot's folder; every path is checked by the host.") {
            row {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    SecondaryButton("Up", enabled = segments.isNotEmpty()) { path = segments.dropLast(1).joinToString("/") }
                    Text("${bot.name}${if (segments.isEmpty()) "" else "/" + segments.joinToString("/")}",
                        style = Type.caption, color = Palette.secondary, modifier = Modifier.weight(1f))
                }
            }
            val list = rows
            when {
                list == null && error == null -> row { Text("Loading…", style = Type.body, color = Palette.secondary) }
                list != null && list.isEmpty() -> row { Text("This folder is empty.", style = Type.body, color = Palette.secondary) }
                list != null -> for (entry in list) {
                    val item = entry.jsonObject
                    val dir = item["dir"]?.jsonPrimitive?.content == "true"
                    val name = item["name"]?.jsonPrimitive?.content.orEmpty()
                    row {
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            IconButton(if (dir) "Open folder" else "Copy path", if (dir) "folder" else "doc") {
                                if (dir) path = (segments + name).joinToString("/") else copyPath(name)
                            }
                            Text(name, style = Type.body, color = Palette.text, modifier = Modifier.weight(1f))
                            Text(if (dir) "Folder" else sizeText(item["size"]?.jsonPrimitive?.content?.toLongOrNull() ?: 0L),
                                style = Type.caption, color = Palette.tertiary)
                        }
                    }
                }
            }
        }
        val shownError = error
        if (shownError != null) {
            CardSection("Status") { row { Text(shownError, style = Type.caption, color = Palette.danger) } }
        }
    }
}
