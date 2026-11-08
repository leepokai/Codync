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
import com.codync.android.core.BotDraft
import com.codync.android.design.*
import kotlinx.coroutines.launch

/** Keys this bot keeps for itself, above its agent's keys. Values stay in the computer. */
@Composable internal fun BotAgentKeys(state: AppState, store: AppStore, draft: BotDraft) {
    val id = draft.id ?: return
    val scope = rememberCoroutineScope()
    var own by remember(id) { mutableStateOf<List<String>?>(null) }
    var inherited by remember(id) { mutableStateOf<List<String>>(emptyList()) }
    var key by remember(id) { mutableStateOf("") }
    var value by remember(id) { mutableStateOf("") }
    var error by remember(id) { mutableStateOf<String?>(null) }
    fun load() = scope.launch {
        try {
            val (mine, theirs) = store.agentEnv(draft.backend, id)
            own = mine; inherited = theirs; error = null
        } catch (e: Exception) { error = e.message }
    }
    LaunchedEffect(id, draft.backend, state.contextId) { load() }
    fun save(variable: String, text: String) = scope.launch {
        try { store.setBotAgentKey(draft.backend, id, variable, text); key = ""; value = ""; load() }
        catch (e: Exception) { error = e.message }
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Keys for this bot. They override the agent keys and stay in this computer.",
            style = Type.caption, color = Palette.secondary)
        for (name in own.orEmpty()) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(name, Modifier.weight(1f), style = Type.caption, color = Palette.text)
            IconButton("Remove $name", "trash") { save(name, "") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            FieldBox(key, { key = it.uppercase().replace(Regex("[^A-Z0-9_]"), "_") }, "VARIABLE", 1, 1,
                fill = Palette.bubbleUser, capitalize = false)
            FieldBox(value, { value = it }, "Value", 1, 1, fill = Palette.bubbleUser, capitalize = false)
            SecondaryButton("Save key", enabled = key.isNotBlank() && value.isNotBlank()) { save(key, value) }
        }
        if (inherited.isNotEmpty()) Text("Agent keys, for every bot: ${inherited.joinToString(", ")}",
            style = Type.caption, color = Palette.tertiary)
        error?.let { Text(it, style = Type.caption, color = Palette.danger) }
    }
}
