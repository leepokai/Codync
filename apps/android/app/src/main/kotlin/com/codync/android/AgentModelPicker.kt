package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentModels
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

/**
 * Models come from the selected computer's agent, never a bundled list (iOS `AgentModelPicker`).
 * The selected model's settings follow it; Fast mode is its own row while the agent offers it. Without [setFast] (a
 * fallback's model), the bot's own session and Fast mode don't apply, so neither shows.
 */
@Composable internal fun AgentModelPicker(state: AppState, load: suspend (String) -> AgentModels, backend: String, selection: String?,
    options: Map<String, String>, fastMode: Boolean, bot: Bot?, choose: (String?, Map<String, String>) -> Unit, setFast: ((Boolean) -> Unit)?) {
    val online = state.actualConnection is LinkState.Ready
    var catalog by remember(backend) { mutableStateOf<AgentModels?>(null) }
    var loading by remember(backend) { mutableStateOf(false) }
    var failure by remember(backend) { mutableStateOf<String?>(null) }
    var retry by remember(backend) { mutableIntStateOf(0) }
    LaunchedEffect(state.computers.firstOrNull()?.id, backend, online, retry) {
        failure = null; loading = false
        if (backend == "custom") return@LaunchedEffect
        if (!online) { failure = "Connect to this computer to load its models. You can still enter a model ID."; return@LaunchedEffect }
        loading = true
        try { catalog = load(backend) }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { failure = "Couldn't load models: ${error.message ?: "Try again."}" }
        finally { loading = false }
    }
    // The agent's own "Default" entry is the same choice as no model.
    val agentDefault = catalog?.agentDefault
    val value = if (selection == agentDefault?.id) "" else selection.orEmpty()
    val modelOptions = catalog?.models?.firstOrNull { it.id == value }?.options.orEmpty()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Model", style = Type.body, color = Palette.text)
            Box(Modifier.weight(1f))
            if (backend != "custom") Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                if (loading) Spinner(12.dp) else IconButton(if (failure == null) "Refresh model list" else "Retry model list", "arrow.clockwise") { retry++ }
            }
            if (backend == "custom" || (!loading && catalog?.models.isNullOrEmpty())) {
                Box(Modifier.widthIn(max = 180.dp).outline().pill(androidx.compose.ui.graphics.Color.Transparent)) {
                    PlainTextField(value, { choose(it.ifEmpty { null }, emptyMap()) }, prompt = "Default", singleLine = true,
                        style = Type.body.copy(textAlign = TextAlign.End))
                }
            } else {
                val choices = catalog?.options(selection)?.map { it.id.orEmpty() to it.name } ?: listOf("" to "Default")
                ChoicePicker(value, choices, { if (it != value) choose(it.ifEmpty { null }, emptyMap()) }, label = "Model")
            }
        }
        for (option in modelOptions) Row(verticalAlignment = Alignment.CenterVertically) {
            Text(option.name, Modifier.weight(1f), style = Type.body, color = Palette.text)
            val fallback = option.values.firstOrNull { it.id == option.default }?.name
            ChoicePicker(options[option.id].orEmpty(), listOf("" to (fallback?.let { "Default · $it" } ?: "Default")) + option.values.map { it.id to it.name },
                { picked -> choose(selection, if (picked.isEmpty()) options - option.id else options + (option.id to picked)) }, label = option.name)
        }
        val note = failure
        if (backend != "custom" && !loading && note != null) Text(note, style = Type.caption, color = Palette.warning)
        else if (backend != "custom" && !loading && catalog?.models?.isEmpty() == true)
            Text("This agent doesn't advertise a model list. Leave Default or enter a model ID.", style = Type.caption, color = Palette.secondary)
        if (setFast != null) {
            Text("Changing the model starts a new agent session. Chat history is kept.", style = Type.caption, color = Palette.secondary)
            val showsFast = (backend != "custom" && catalog?.fastMode == true) || fastMode || (bot?.backend == backend && bot.fastModeShown)
            if (showsFast) SwitchRow("Fast mode", bot?.fastModeRefusal ?: "Faster replies for more usage, on models that offer it.", fastMode, setFast)
        }
    }
}
