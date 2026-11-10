package com.codync.android

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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

private fun channelFilterKey(provider: String) = if (provider == "telegram") "chat" else "channel"

private suspend fun AppStore.channelRoutines(botId: String): JsonArray =
    hostQuery("routines", buildJsonObject { put("botId", botId) }).jsonObject["routines"]?.jsonArray ?: JsonArray(emptyList())

private suspend fun AppStore.createChannelRoutine(botId: String, provider: String, channel: String, name: String, instruction: String) {
    hostQuery("saveRoutine", buildJsonObject {
        put("botId", botId)
        put("name", name)
        put("instruction", instruction)
        put("timeoutSeconds", 600)
        put("schedule", buildJsonObject {
            put("kind", "event")
            put("eventSource", provider)
            put("eventName", "*")
            put("eventFilters", buildJsonObject { put(channelFilterKey(provider), channel) })
            put("zone", "UTC")
        })
    })
}

private fun JsonObject.listensTo(provider: String, channel: String): Boolean {
    val triggers = this["triggers"]?.jsonArray ?: return false
    return triggers.any { trigger ->
        val row = trigger.jsonObject
        if (row["source"]?.jsonPrimitive?.content != provider) return@any false
        val filters = row["filters"]?.jsonObject ?: return@any false
        filters[channelFilterKey(provider)]?.jsonPrimitive?.content == channel
    }
}

/** The routines that listen to this channel, and a short way to make one from the phone. */
@Composable internal fun ChannelRoutinesRow(provider: String, channel: String, botId: String, store: AppStore) {
    val scope = rememberCoroutineScope()
    var routines by remember(botId, channel) { mutableStateOf<List<JsonObject>>(emptyList()) }
    var open by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    var instruction by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun load() = scope.launch {
        routines = runCatching {
            store.channelRoutines(botId).map { it.jsonObject }.filter { it.listensTo(provider, channel) }
        }.getOrDefault(emptyList())
    }
    LaunchedEffect(botId, provider, channel) { load() }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Routines that listen to this channel", Modifier.weight(1f), style = Type.caption, color = Palette.tertiary)
            IconButton("New routine for this channel", "plus.circle") { open = !open }
        }
        for (routine in routines) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(routine["name"]?.jsonPrimitive?.content.orEmpty(), Modifier.weight(1f), style = Type.caption, color = Palette.text)
                Text(if (routine["enabled"]?.jsonPrimitive?.content == "true") "On" else "Off", style = Type.caption, color = Palette.secondary)
            }
        }
        if (routines.isEmpty() && !open) Text("None yet.", style = Type.caption, color = Palette.tertiary)
        if (open) {
            FieldBox(name, { name = it }, "Routine name", 1, 1, fill = Palette.bubbleUser)
            FieldBox(instruction, { instruction = it }, "What it should do when this channel mentions the bot", 1, 3, fill = Palette.bubbleUser)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton("Create", enabled = !busy && name.isNotBlank() && instruction.isNotBlank()) {
                    busy = true
                    scope.launch {
                        try {
                            store.createChannelRoutine(botId, provider, channel, name.trim(), instruction.trim())
                            name = ""; instruction = ""; open = false; error = null; load()
                        } catch (e: Exception) { error = e.message } finally { busy = false }
                    }
                }
                SecondaryButton("Cancel") { open = false }
            }
        }
        error?.let { Text(it, style = Type.caption, color = Palette.danger) }
    }
}
