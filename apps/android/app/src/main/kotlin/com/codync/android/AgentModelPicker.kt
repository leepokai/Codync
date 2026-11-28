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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.readyLogins
import com.codync.android.core.AgentModels
import com.codync.android.core.Bot
import com.codync.android.core.Login
import com.codync.android.core.LoginSeat
import com.codync.android.core.listValue
import com.codync.android.core.loginSeats
import com.codync.android.core.selected
import com.codync.android.core.settings
import com.codync.android.core.showsFast
import com.codync.android.design.*

private enum class PickerStage { Closed, Agents, Models, Settings }

/** A row of the agent or model list (iOS `AgentModelPicker.Choice`). */
private class Choice(val name: String, val on: Boolean, val pick: () -> Unit)

/**
 * Models come from the selected computer's agent, never a bundled list (iOS `AgentModelPicker`).
 * The selected model's settings follow it; Fast mode is its own row while the agent offers it. Without [setFast] (a
 * fallback's model), the bot's own session and Fast mode don't apply, so neither shows. The agent is picked as one of its
 * logins ([agents]); with [store], the list ends on "Add login profile…" for an agent that has login profiles.
 */
@Composable internal fun AgentModelPicker(state: AppState, load: ModelLoad, backend: String, selection: String?,
    options: Map<String, String>, fastMode: Boolean, bot: Bot?, choose: (String?, Map<String, String>) -> Unit, setFast: ((Boolean) -> Unit)?,
    agents: List<LoginSeat> = emptyList(), chooseAgent: ((Login) -> Unit)? = null,
    onReload: () -> Unit = {}, expanded: Boolean = false, loginProfile: String? = null, store: AppStore? = null, agentDetails: @Composable () -> Unit = {}) {
    val models = rememberModelCatalog(state, load, backend, loginProfile)
    val catalog = models.catalog
    // expanded: a fallback just added starts open on its choices.
    var stage by remember { mutableStateOf(if (expanded) PickerStage.Settings else PickerStage.Closed) }
    var query by remember { mutableStateOf("") }
    var adding by remember { mutableStateOf(false) }
    val current = Login(backend, loginProfile)
    val addsLogin = store?.let { agentBackends(state).firstOrNull { it.id == backend }?.takeIf { it.loginProfiles != null } }
    fun open(next: PickerStage) { query = ""; stage = next }
    // The agent's own "Default" entry is the same choice as no model.
    val value = catalog.listValue(selection)
    val selectedModel = catalog.selected(selection)
    val modelOptions = catalog.settings(selection)
    val modelChoices = catalog?.options(selection)?.map { it.id.orEmpty() to it.name } ?: listOf("" to "Default")
    val agentName = agents.firstOrNull { it.login == current }?.name ?: backend
    val modelName = if (value.isEmpty()) "Default" else modelChoices.firstOrNull { it.first == value }?.second ?: value
    val levels = modelOptions.mapNotNull { option -> option.values.firstOrNull { it.id == (options[option.id] ?: option.default) }?.name }
    val summary = (listOf(agentName, modelName) + levels + if (fastMode) listOf("Fast") else emptyList()).joinToString(" · ")
    val unfold = tr(if (stage == PickerStage.Closed) "Collapsed" else "Expanded")
    Column(Modifier.fillMaxWidth().animateContentSize(Motion.layoutSize), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth().clickable { open(if (stage == PickerStage.Closed) PickerStage.Models else PickerStage.Closed) }
            .semantics { stateDescription = unfold }.padding(vertical = 8.dp),
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
                    if (models.loading) Spinner(12.dp) else IconButton(if (models.failure == null) "Refresh model list" else "Retry model list", "arrow.clockwise") { models.retry++; onReload() }
                }
                val rows = if (stage == PickerStage.Agents) agents.map { seat ->
                    Choice(seat.name, seat.login == current) { if (seat.login != current) chooseAgent?.invoke(seat.login); open(PickerStage.Models) }
                } else modelChoices.map { (id, name) ->
                    Choice(name, id == value) { if (id != value) choose(id.ifEmpty { null }, emptyMap()); open(PickerStage.Settings) }
                }
                val choices = rows.filter { it.name.contains(query, ignoreCase = true) }
                val scroll = rememberScrollState()
                // The rows blur under the edges into the card's fill (an outlined card has none: the page shows).
                val ground = if (LocalCardStyle.current == CardStyle.Outlined) Palette.background else Palette.surface
                ScrollEdges(scroll, Modifier.heightIn(max = 180.dp), ground) { Column(Modifier.verticalScroll(scroll)) {
                    for (choice in choices) Row(Modifier.fillMaxWidth().clickable(onClick = choice.pick)
                        .padding(horizontal = 6.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(choice.name, Modifier.weight(1f), style = Type.body, color = Palette.text)
                        if (choice.on) Symbol("checkmark", size = 12.dp, tint = Palette.text)
                    }
                    if (choices.isEmpty()) Text("No results", style = Type.caption, color = Palette.secondary)
                    if (stage == PickerStage.Agents && addsLogin != null) Row(Modifier.fillMaxWidth().clickable { adding = true }
                        .padding(horizontal = 6.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(tr("Add login profile…"), Modifier.weight(1f), style = Type.body, color = Palette.secondary)
                        Symbol("plus", size = 12.dp, tint = Palette.secondary)
                    }
                } }
                if (stage == PickerStage.Models && (backend == "custom" || (!models.loading && catalog?.models.isNullOrEmpty())))
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
            models.failure?.let { Text(it, style = Type.caption, color = Palette.warning) }
            if (setFast != null) {
                if (catalog.showsFast(backend, fastMode, bot)) SwitchRow("Fast mode", bot?.fastModeRefusal.orEmpty(), fastMode, setFast)
                Text("Changing the model starts a new agent session. Chat history is kept.", style = Type.caption, color = Palette.secondary)
            }
        }
    }
    if (store != null && addsLogin != null) CodyncSheet(adding, { adding = false }) { AgentSheet(addsLogin, state, store) }
}

/** [agents] as the Agent pickers list them, one row per login ("(not installed)" after an agent that isn't), in the app's language. */
@Composable internal fun pickerSeats(agents: List<AgentBackend>, on: Login, logins: ((AgentBackend) -> List<String?>?)? = null): List<LoginSeat> =
    agents.map { if (it.available) it else it.copy(name = "${it.name} (not installed)") }.loginSeats(on, tr("Default login profile"), tr("New login profile"), logins)

/** The logins a chat's box can switch to (iOS `BotStore.readySeats`, the desktop's `readySeats`): the installed agents and the one [on]. */
@Composable internal fun readySeats(state: AppState, on: Login): List<LoginSeat> =
    pickerSeats(agentBackends(state).filter { it.available || it.id == on.backend }, on) { it.readyLogins(on) }

