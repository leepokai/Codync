package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentModels
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

private enum class PickerStage { Closed, Agents, Models, Settings }

/**
 * Models come from the selected computer's agent, never a bundled list (iOS `AgentModelPicker`).
 * The selected model's settings follow it; Fast mode is its own row while the agent offers it. Without [setFast] (a
 * fallback's model), the bot's own session and Fast mode don't apply, so neither shows.
 */
@Composable internal fun AgentModelPicker(state: AppState, load: suspend (String) -> AgentModels, backend: String, selection: String?,
    options: Map<String, String>, fastMode: Boolean, bot: Bot?, choose: (String?, Map<String, String>) -> Unit, setFast: ((Boolean) -> Unit)?,
    agents: List<Pair<String, String>> = emptyList(), chooseAgent: ((String) -> Unit)? = null,
    agentDetails: @Composable () -> Unit = {}) {
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
    var stage by remember { mutableStateOf(PickerStage.Closed) }
    var query by remember { mutableStateOf("") }
    fun open(next: PickerStage) { query = ""; stage = next }
    // The agent's own "Default" entry is the same choice as no model.
    val agentDefault = catalog?.agentDefault
    val value = if (selection == agentDefault?.id) "" else selection.orEmpty()
    val selectedModel = catalog?.models?.firstOrNull { it.id == (selection ?: agentDefault?.id ?: catalog?.currentModelId) }
    val modelOptions = if (value.isNotEmpty() || catalog?.defaultModelOptions == true) selectedModel?.options.orEmpty() else emptyList()
    val modelChoices = catalog?.options(selection)?.map { it.id.orEmpty() to it.name } ?: listOf("" to "Default")
    val agentName = agents.firstOrNull { it.first == backend }?.second ?: backend
    val modelName = if (value.isEmpty()) "Default" else modelChoices.firstOrNull { it.first == value }?.second ?: value
    val levels = modelOptions.mapNotNull { option -> option.values.firstOrNull { it.id == (options[option.id] ?: option.default) }?.name }
    val summary = (listOf(agentName, modelName) + levels + if (fastMode) listOf("Fast") else emptyList()).joinToString(" · ")
    Column(Modifier.fillMaxWidth().animateContentSize(Motion.layoutSize), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth().clickable { open(if (stage == PickerStage.Closed) PickerStage.Models else PickerStage.Closed) }.padding(vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Crossfade(summary, Modifier.weight(1f), animationSpec = Motion.fade(), label = "Agent summary") { text ->
                Text(text, style = Type.subheadline, color = Palette.text)
            }
            Symbol(if (stage == PickerStage.Closed) "chevron.down" else "chevron.up", size = 12.dp, tint = Palette.secondary)
        }
        if (stage != PickerStage.Closed) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (chooseAgent != null) LinkButton(agentName, Modifier.weight(1f)) { open(if (stage == PickerStage.Agents) PickerStage.Settings else PickerStage.Agents) }
                LinkButton(modelName, Modifier.weight(1f)) { open(if (stage == PickerStage.Models) PickerStage.Settings else PickerStage.Models) }
            }
            agentDetails()
            if (stage == PickerStage.Agents || stage == PickerStage.Models) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (stage == PickerStage.Models && chooseAgent != null) IconButton("Back", "chevron.left") { open(PickerStage.Agents) }
                    Box(Modifier.weight(1f)) { PlainTextField(query, { query = it }, prompt = if (stage == PickerStage.Agents) "Search agents" else "Search models", singleLine = true) }
                    if (loading) Spinner(12.dp) else IconButton("Refresh model list", "arrow.clockwise") { retry++ }
                }
                val choices = (if (stage == PickerStage.Agents) agents else modelChoices).filter { it.second.contains(query, ignoreCase = true) }
                Column(Modifier.heightIn(max = 180.dp).verticalScroll(rememberScrollState())) {
                    for ((id, name) in choices) Row(Modifier.fillMaxWidth().clickable {
                        if (stage == PickerStage.Agents) { if (id != backend) chooseAgent?.invoke(id); open(PickerStage.Models) }
                        else { if (id != value) choose(id.ifEmpty { null }, emptyMap()); open(PickerStage.Settings) }
                    }.padding(horizontal = 6.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(name, Modifier.weight(1f), style = Type.body, color = Palette.text)
                        if (id == if (stage == PickerStage.Agents) backend else value) Symbol("checkmark", size = 12.dp, tint = Palette.text)
                    }
                    if (choices.isEmpty()) Text("No results", style = Type.caption, color = Palette.secondary)
                }
                if (stage == PickerStage.Models && (backend == "custom" || (!loading && catalog?.models.isNullOrEmpty())))
                    PlainTextField(value, { choose(it.ifEmpty { null }, emptyMap()) }, prompt = "Default", singleLine = true)
            }
            for (option in modelOptions) {
                if (option.isReasoning) {
                    ReasoningSlider(option, options[option.id]) { level ->
                        choose(selection, if (level == null) options - option.id else options + (option.id to level))
                    }
                } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(option.name, Modifier.weight(1f), style = Type.caption, color = Palette.secondary)
                    val fallback = option.values.firstOrNull { it.id == option.default }?.name
                    LinkButton("Default" + (fallback?.let { " · $it" } ?: "") + if (options[option.id] == null) " ✓" else "",
                        style = Type.caption) { choose(selection, options - option.id) }
                }
                if (option.values.isNotEmpty()) Row(Modifier.horizontalScroll(rememberScrollState())) {
                    SegmentedChoice(options[option.id] ?: option.default.orEmpty(), option.values.map { it.id to it.name },
                        { choose(selection, options + (option.id to it)) }, Modifier.width((option.values.size * 75).dp))
                }
            }
            }
            selectedModel?.optionsError?.let { Text(it, style = Type.caption, color = Palette.warning) }
            failure?.let { Text(it, style = Type.caption, color = Palette.warning) }
            if (setFast != null) {
                val showsFast = (backend != "custom" && catalog?.fastMode == true) || fastMode || (bot?.backend == backend && bot.fastModeShown)
                if (showsFast) SwitchRow("Fast mode", bot?.fastModeRefusal.orEmpty(), fastMode, setFast)
                Text("Changing the model starts a new agent session. Chat history is kept.", style = Type.caption, color = Palette.secondary)
            }
        }
    }
}
