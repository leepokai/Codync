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
import com.codync.android.core.EffortLevel
import com.codync.android.core.ModelOption
import com.codync.android.core.SessionReport
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

/**
 * Models come from the selected computer's agent, never a bundled list (iOS `AgentModelPicker`).
 * The selected model's settings follow it: the Effort level row first (the agent's `thought_level` setting), then the
 * agent's others under their own names. Fast mode is its own row while the agent offers it. Without [setFast] (a
 * fallback's model), the bot's own session and Fast mode don't apply, so neither shows. [status]: what this step's own
 * agent reports while it runs and what the host saved, for the Effort level's line (null: no line, as for an unsaved
 * agent or model); [working]: the bot is replying, so a change waits for the reply.
 */
@Composable internal fun AgentModelPicker(state: AppState, load: suspend (String) -> AgentModels, backend: String, selection: String?,
    options: Map<String, String>, fastMode: Boolean, bot: Bot?, choose: (String?, Map<String, String>) -> Unit, setFast: ((Boolean) -> Unit)?,
    status: Pair<SessionReport?, Map<String, String>>? = null, working: Boolean = false) {
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
    val modelOptions = catalog?.settings(value).orEmpty()
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
        val setOption = { option: ModelOption, picked: String ->
            choose(selection, if (picked.isEmpty()) options - option.id else options + (option.id to picked))
        }
        fun effort(saved: String?) = EffortLevel.of(modelOptions, saved, status?.first, loading, failure != null, value.isNotEmpty())
        val effortId = (effort(null) as? EffortLevel.Row)?.setting?.id
        when (val level = effort(effortId?.let { status?.second?.get(it) })) {
            EffortLevel.Later -> Text("Effort level appears once the agent runs this model.", style = Type.caption, color = Palette.secondary)
            is EffortLevel.Row -> Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                level.setting?.let { SettingRow("Effort level", it, options[it.id].orEmpty()) { picked -> setOption(it, picked) } }
                // Nothing to report for a value not saved yet.
                val caption = level.caption?.takeIf { status != null && effortId != null && status.second[effortId] == options[effortId] }
                if (caption != null) Text(caption, style = Type.caption, color = Palette.secondary)
            }
            EffortLevel.None -> {}
        }
        for (option in modelOptions.filter { it.id != effortId }) SettingRow(option.name, option, options[option.id].orEmpty()) { setOption(option, it) }
        val note = failure
        if (backend != "custom" && !loading && note != null) Text(note, style = Type.caption, color = Palette.warning)
        else if (backend != "custom" && !loading && catalog?.models?.isEmpty() == true)
            Text("This agent doesn't advertise a model list. Leave Default or enter a model ID.", style = Type.caption, color = Palette.secondary)
        if (setFast != null) {
            Text(if (working) "Changes to the model or effort level apply after this reply, in a new agent session. Chat history is kept; the agent’s session context starts fresh."
                else "Changes to the model or effort level apply to the next message, in a new agent session. Chat history is kept; the agent’s session context starts fresh.",
                style = Type.caption, color = Palette.secondary)
            val showsFast = (backend != "custom" && catalog?.fastMode == true) || fastMode || (bot?.backend == backend && bot.fastModeShown)
            if (showsFast) SwitchRow("Fast mode", bot?.fastModeRefusal ?: "Faster replies for more usage, on models that offer it.", fastMode, setFast)
        }
    }
}

/** One setting the agent advertises: Default (its own, as last checked), then only the values it lists. */
@Composable private fun SettingRow(label: String, option: ModelOption, value: String, pick: (String) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f), style = Type.body, color = Palette.text)
        val fallback = option.values.firstOrNull { it.id == option.default }?.name
        ChoicePicker(value, listOf("" to (fallback?.let { "Default · $it" } ?: "Default")) + option.values.map { it.id to it.name }, pick, label = label)
    }
}
