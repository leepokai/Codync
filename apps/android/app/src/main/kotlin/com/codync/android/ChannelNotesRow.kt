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

/** Small facts this channel keeps, given to the bot with its memory before every message. */
@Composable internal fun ChannelNotesRow(provider: String, channel: String, store: AppStore) {
    val scope = rememberCoroutineScope()
    var notes by remember(provider, channel) { mutableStateOf<List<String>>(emptyList()) }
    var text by remember(provider, channel) { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(provider, channel) {
        notes = runCatching { store.channelNotes(provider, channel) }.getOrDefault(emptyList())
    }
    fun run(work: suspend () -> List<String>) = scope.launch {
        busy = true
        try { notes = work(); error = null } catch (e: Exception) { error = e.message } finally { busy = false }
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Notes", style = Type.caption, color = Palette.tertiary)
        notes.forEachIndexed { index, note ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(note, Modifier.weight(1f), style = Type.caption, color = Palette.text)
                IconButton("Remove note", "minus.circle", enabled = !busy) {
                    run { store.removeChannelNote(provider, channel, index) }
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            FieldBox(text, { text = it }, "A fact this channel should keep", 1, 1, fill = Palette.bubbleUser, capitalize = false)
            SecondaryButton("Add", enabled = !busy && text.isNotBlank()) {
                val note = text.trim()
                run { store.addChannelNote(provider, channel, note).also { text = "" } }
            }
        }
        error?.let { Text(it, style = Type.caption, color = Palette.danger) }
    }
}
