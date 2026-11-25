package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.codync.android.core.AgentModels
import com.codync.android.core.Bot
import com.codync.android.core.Login
import com.codync.android.core.LoginSeat
import com.codync.android.core.ModelOption
import com.codync.android.core.listValue
import com.codync.android.core.selected
import com.codync.android.core.settings
import com.codync.android.core.showsFast
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/**
 * The desktop's model panel (`ModelPicker`) as a sheet's body (iOS `ModelPanel`). On the left is a rail of the agents'
 * logins, where a tap switches agent and a "+" adds a login profile to an agent that has them. On the right are the
 * agent's models under a search field, with a check on the one in use; a typed ID the agent doesn't list is offered as a
 * model ID. Under the models are the model's effort (a slider) and its other settings (menus). The footer holds
 * Standard / Fast when the agent has Fast mode ([setFast] set), then [notes]. Picking a model keeps the panel open, so its
 * settings can follow; another model starts without settings. [bot] is nil for a thread without one.
 */
@Composable internal fun ModelPanel(state: AppState, store: AppStore, backend: String, loginProfile: String?, model: String?,
    options: Map<String, String>, choose: (String?, Map<String, String>) -> Unit, fast: Boolean?, setFast: ((Boolean) -> Unit)?,
    bot: Bot?, seats: List<LoginSeat>, pickSeat: (Login) -> Unit, notes: List<Pair<String, Color>> = emptyList(), modifier: Modifier = Modifier) {
    val models = rememberModelCatalog(state, store::agentModels, backend, loginProfile)
    val catalog = models.catalog
    var query by remember { mutableStateOf("") }
    // The model in use as the list has it: "" for Default.
    val value = catalog.listValue(model)
    val choices = (catalog ?: AgentModels(emptyList())).options(model).map { it.id.orEmpty() to tr(it.name) }
    val typed = query.trim()
    val found = if (typed.isEmpty()) choices else choices.filter { it.second.contains(typed, ignoreCase = true) }
    // What's typed, offered as a model ID when nothing listed has it.
    val custom = typed.takeUnless { t -> t.isEmpty() || choices.any { it.first == t || it.second == t } }
    val listed = !catalog?.models.isNullOrEmpty()
    val empty = if (models.loading || custom != null || (listed && found.isNotEmpty())) null
        else models.failure ?: if (listed) "No results" else "This agent doesn't advertise a model list. Leave Default or enter a model ID."
    val showsFast = setFast != null && catalog.showsFast(backend, fast == true, bot)
    fun pick(id: String) { if (id != value) choose(id.ifEmpty { null }, emptyMap()) }

    Column(modifier.fillMaxSize()) {
        Row(Modifier.weight(1f).fillMaxWidth()) {
            SeatRail(state, store, backend, Login(backend, loginProfile), seats, pickSeat)
            Column(Modifier.weight(1f).fillMaxHeight()) {
                ModelSearch(query, { query = it }, models)
                ModelList(state, backend, found, value, custom, empty, Modifier.weight(1f), pick = ::pick, pickCustom = { pick(it); query = "" })
                ModelSettings(catalog.settings(model), catalog.selected(model)?.optionsError, options) { choose(model, it) }
            }
        }
        if (showsFast || notes.isNotEmpty()) PanelFooter(if (showsFast) setFast else null, fast == true, bot?.fastModeRefusal, notes)
    }
}

/** The agents' logins, each its logo; the one in use ([current]) has a bar at the rail's edge, and "+" adds a login profile. */
@Composable private fun SeatRail(state: AppState, store: AppStore, backend: String, current: Login, seats: List<LoginSeat>, pickSeat: (Login) -> Unit) {
    var adding by remember { mutableStateOf(false) }
    val addsLogin = agentBackends(state).firstOrNull { it.id == backend && it.loginProfiles != null }
    Column(Modifier.width(52.dp).fillMaxHeight().background(Palette.surface).verticalScroll(rememberScrollState()).padding(4.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        for (seat in seats) {
            val on = seat.login == current
            Box(Modifier.size(44.dp).clickable(role = Role.Button) { if (!on) pickSeat(seat.login) }
                .semantics { contentDescription = seat.name; this.selected = on }, contentAlignment = Alignment.Center) {
                AgentIcon(agentRegistry(state, seat.login.backend), 22.dp)
                // A login profile beside the agent's default login: its name's first letter at the logo's corner.
                if (seat.login.loginProfile != null) seat.name.substringAfterLast(" · ").firstOrNull()?.let { initial ->
                    Box(Modifier.align(Alignment.BottomEnd).padding(4.dp).defaultMinSize(14.dp, 14.dp).clip(RoundedRectangle(4.dp))
                        .background(Palette.text), contentAlignment = Alignment.Center) {
                        Text(initial.uppercase(), style = Type.caption.copy(fontSize = 9.sp, fontWeight = FontWeight.Bold), color = Palette.background)
                    }
                }
                if (on) Box(Modifier.align(Alignment.CenterEnd).offset(x = 4.dp).size(3.dp, 20.dp).clip(Capsule()).background(Palette.text))
            }
        }
        if (addsLogin != null) IconButton("Add login profile", "plus") { adding = true }
    }
    if (addsLogin != null) CodyncSheet(adding, { adding = false }) { AgentSheet(addsLogin, state, store) }
}

/** The search field over the models, with Refresh (a spinner while they load). */
@Composable private fun ModelSearch(query: String, setQuery: (String) -> Unit, models: ModelCatalog) {
    Row(Modifier.fillMaxWidth().hairline(Palette.border, bottom = true).padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol("magnifyingglass", size = 12.dp, tint = Palette.tertiary)
        Box(Modifier.weight(1f)) { PlainTextField(query, setQuery, prompt = "Search models", singleLine = true) }
        Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            if (models.loading) Spinner(12.dp)
            else IconButton(if (models.failure == null) "Refresh model list" else "Retry model list", "arrow.clockwise") { models.retry++ }
        }
    }
}

/** The models [found] (by id and name, [value] checked), then [custom] as a model ID, or why there is nothing ([empty]). */
@Composable private fun ModelList(state: AppState, backend: String, found: List<Pair<String, String>>, value: String, custom: String?,
    empty: String?, modifier: Modifier, pick: (String) -> Unit, pickCustom: (String) -> Unit) {
    LazyColumn(modifier.fillMaxWidth(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        items(found, key = { it.first }) { (id, name) ->
            val on = id == value
            Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clip(RoundedRectangle(10.dp))
                .background(if (on) Palette.bubbleAgent else Color.Transparent).clickable(role = Role.Button) { pick(id) }
                .semantics { this.selected = on }.padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AgentIcon(agentRegistry(state, backend), 16.dp)
                Text(name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (on) Symbol("checkmark", size = 12.dp, tint = Palette.text)
            }
        }
        if (custom != null) item {
            Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(role = Role.Button) { pickCustom(custom) }
                .padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Symbol("plus", size = 12.dp, tint = Palette.secondary)
                Text(custom, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.weight(1f))
                Text(tr("Model ID"), style = Type.caption, color = Palette.tertiary)
            }
        }
        if (empty != null) item { Text(tr(empty), Modifier.padding(10.dp), style = Type.caption, color = Palette.tertiary) }
    }
}

/** The model's effort as a slider, then its other [settings], each a menu of its values; [setOptions] takes them all. */
@Composable private fun ModelSettings(settings: List<ModelOption>, error: String?, options: Map<String, String>, setOptions: (Map<String, String>) -> Unit) {
    if (settings.isEmpty() && error == null) return
    Column(Modifier.fillMaxWidth().hairline(Palette.border, bottom = false).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        for (option in settings) {
            val chosen = options[option.id] ?: option.default
            if (option.isReasoning) {
                ReasoningSlider(option, options[option.id]) { level ->
                    setOptions(if (level == null) options - option.id else options + (option.id to level))
                }
            } else DropdownMenu({ option.values.map { item ->
                MenuItem(item.name, selected = item.id == chosen) { setOptions(options + (option.id to item.id)) }
            } }, Modifier.fillMaxWidth()) {
                Row(Modifier.fillMaxWidth().heightIn(min = 36.dp), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(option.name, Modifier.weight(1f), style = Type.subheadline, color = Palette.secondary)
                    Text(option.values.firstOrNull { it.id == chosen }?.name ?: tr("Default"), style = Type.subheadline,
                        color = Palette.text, maxLines = 1)
                    Symbol("chevron.up.chevron.down", size = 10.dp, tint = Palette.tertiary)
                }
            }
        }
        error?.let { Text(it, style = Type.caption, color = Palette.warning) }
    }
}

/** Standard / Fast when [setFast] is given ([refusal]: why the agent turned Fast down), then [notes]. */
@Composable private fun PanelFooter(setFast: ((Boolean) -> Unit)?, fast: Boolean, refusal: String?, notes: List<Pair<String, Color>>) {
    Column(Modifier.fillMaxWidth().background(Palette.surface).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (setFast != null) {
            SegmentedChoice(fast, listOf(false to "Standard", true to "Fast"), setFast)
            refusal?.let { Text(tr(it), style = Type.caption, color = Palette.secondary) }
        }
        for ((text, tone) in notes) Text(text, style = Type.caption, color = tone)
    }
}

/** A 1 dp line in [color] along the bottom (or top), inset 12 dp on each side. */
private fun Modifier.hairline(color: Color, bottom: Boolean, inset: Dp = 12.dp) = drawBehind {
    val y = if (bottom) size.height - 1.dp.toPx() else 0f
    drawRect(color, Offset(inset.toPx(), y), Size(size.width - 2 * inset.toPx(), 1.dp.toPx()))
}
