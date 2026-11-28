package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentModels
import com.codync.android.core.ApprovalChoice
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.Login
import com.codync.android.core.LoginSeat
import com.codync.android.core.ModelOption
import com.codync.android.core.listValue
import com.codync.android.core.selected
import com.codync.android.core.settings
import com.codync.android.core.showsFast
import com.codync.android.core.withApproval
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

/** How many models show before "More models": an agent lists its newest first. */
private const val NEWEST = 6

/**
 * The model panel on a phone, as a sheet's body (iOS `ModelPanel`). At the top the agent in use (a menu of the agents'
 * logins, and adding a login profile to an agent that has them) and a search button. Then the agent's models, the newest
 * ones first and the rest under "More models", each with what it's for and a check on the one in use; a typed ID the
 * agent doesn't list is offered as a model ID, and a pull reads the list from the agent again. Under them one card holds
 * the model's effort (a slider) and its other settings, the approvals (where [approvals] is given) and Fast mode (where
 * [setFast] is given and the agent has it), then [notes]. Picking a model keeps the panel open, so its settings can follow;
 * another model starts without settings. [bot] is nil for a thread without one.
 */
@Composable internal fun ModelPanel(state: AppState, store: AppStore, backend: String, loginProfile: String?, model: String?,
    options: Map<String, String>, choose: (String?, Map<String, String>) -> Unit, fast: Boolean?, setFast: ((Boolean) -> Unit)?,
    bot: Bot?, seats: List<LoginSeat>, pickSeat: (Login) -> Unit, notes: List<Pair<String, Color>> = emptyList(),
    approvals: BotDraft? = null, setApprovals: (BotDraft) -> Unit = {}, modifier: Modifier = Modifier) {
    val models = rememberModelCatalog(state, store::agentModels, backend, loginProfile)
    val catalog = models.catalog
    var query by remember { mutableStateOf("") }
    var searching by remember { mutableStateOf(false) }
    var showsOlder by remember { mutableStateOf(false) }
    // The model in use as the list has it: "" for Default.
    val value = catalog.listValue(model)
    val choices = (catalog ?: AgentModels(emptyList())).options(model).map { it.id.orEmpty() to tr(it.name) }
    val typed = query.trim()
    val found = if (typed.isEmpty()) choices else choices.filter { it.second.contains(typed, ignoreCase = true) }
    // What's typed, offered as a model ID when nothing listed has it.
    val custom = typed.takeUnless { t -> t.isEmpty() || choices.any { it.first == t || it.second == t } }
    // The models under "More models"; none while searching.
    val older = if (typed.isEmpty()) found.drop(NEWEST) else emptyList()
    val shown = if (typed.isEmpty() && !showsOlder) found.take(NEWEST) else found
    val listed = !catalog?.models.isNullOrEmpty()
    // Why reading the list failed (the list kept before stays above it), or why it shows nothing (more).
    val empty = if (models.loading) null else models.failure
        ?: if (custom != null || (listed && found.isNotEmpty())) null
        else if (listed) "No results" else "This agent doesn't advertise a model list. Leave Default or enter a model ID."
    val showsFast = setFast != null && catalog.showsFast(backend, fast == true, bot)
    fun pick(id: String) { if (id != value) choose(id.ifEmpty { null }, emptyMap()) }
    // The one in use is never folded away.
    LaunchedEffect(catalog == null) { if (found.drop(NEWEST).any { it.first == value }) showsOlder = true }

    Column(modifier.fillMaxSize()) {
        PanelTop(state, store, backend, Login(backend, loginProfile), seats, pickSeat, loadingFirst = models.loading && catalog == null,
            searching = searching) { searching = !searching; if (!searching) query = "" }
        AnimatedVisibility(searching, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            SearchField(query, { query = it }, Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp), prompt = "Search or enter a model ID",
                autoFocus = true)
        }
        // Pull down: the computer reads the list from the agent again; the indicator stays until it has.
        PullToRefresh(models::refresh, Modifier.weight(1f)) {
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 8.dp, end = 8.dp, bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp)) {
                items(shown, key = { it.first }) { (id, name) ->
                    ModelRow(name, catalog?.models?.firstOrNull { it.id == id }?.description, on = id == value) { pick(id) }
                }
                if (older.isNotEmpty()) item {
                    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(role = Role.Button) { showsOlder = !showsOlder }
                        .padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(if (showsOlder) tr("Fewer models") else tr("More models (%d)").format(older.size), style = Type.subheadline,
                            color = Palette.secondary)
                        Symbol(if (showsOlder) "chevron.up" else "chevron.down", size = 10.dp, tint = Palette.secondary)
                    }
                }
                if (custom != null) item {
                    Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).clickable(role = Role.Button) { pick(custom); query = "" }
                        .padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Symbol("plus", size = 12.dp, tint = Palette.secondary)
                        Text(custom, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Spacer(Modifier.weight(1f))
                        Text(tr("Model ID"), style = Type.caption, color = Palette.tertiary)
                    }
                }
                if (empty != null) item { Text(tr(empty), Modifier.padding(12.dp), style = Type.caption, color = Palette.tertiary) }
            }
        }
        // While searching, the list gets the room the keyboard leaves.
        if (!searching) SettingsCard(catalog, model, options, { choose(model, it) }, approvals, setApprovals, if (showsFast) setFast else null,
            fast == true, bot?.fastModeRefusal)
        if (!searching && notes.isNotEmpty()) Column(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((text, tone) in notes) Text(text, style = Type.caption, color = tone)
        }
    }
}

/** The agent in use ([current]), a menu of the agents' logins and "Add login profile"; then a spinner while the first list loads, and search. */
@Composable private fun PanelTop(state: AppState, store: AppStore, backend: String, current: Login, seats: List<LoginSeat>, pickSeat: (Login) -> Unit,
    loadingFirst: Boolean, searching: Boolean, toggleSearch: () -> Unit) {
    var adding by remember { mutableStateOf(false) }
    val addsLogin = agentBackends(state).firstOrNull { it.id == backend && it.loginProfiles != null }
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        DropdownMenu({
            seats.map { seat -> MenuItem(seat.name, selected = seat.login == current, verbatim = true) { if (seat.login != current) pickSeat(seat.login) } } +
                listOfNotNull(addsLogin?.let { MenuItem("Add login profile", icon = "plus", divider = true) { adding = true } })
        }, Modifier.clip(Capsule()), label = "Agent") {
            ChipLabel {
                AgentIcon(agentRegistry(state, backend), 18.dp)
                Text(seats.firstOrNull { it.login == current }?.name ?: backend, style = Type.subheadline, color = Palette.secondary, maxLines = 1,
                    overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.weight(1f))
        if (loadingFirst) Spinner(14.dp, Modifier.semantics { contentDescription = "Loading available models" })
        IconButton(if (searching) "Close search" else "Search models", if (searching) "xmark" else "magnifyingglass", action = toggleSearch)
    }
    if (addsLogin != null) CodyncSheet(adding, { adding = false }) { AgentSheet(addsLogin, state, store) }
}

/** A model: its [name] and what it's for ([about]), checked and filled when it's the one in use ([on]). */
@Composable private fun ModelRow(name: String, about: String?, on: Boolean, pick: () -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).clip(RoundedRectangle(14.dp))
        .background(if (on) Palette.bubbleAgent else Color.Transparent).clickable(role = Role.Button, onClick = pick)
        .semantics { this.selected = on }.padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (!about.isNullOrEmpty()) Text(about, style = Type.footnote, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        Symbol("checkmark", Modifier.alpha(if (on) 1f else 0f), size = 14.dp, tint = Palette.text)
    }
}

/**
 * The model's effort as a slider and its other settings as menus ([setOptions] takes them all), then [approvals] and Fast
 * mode (when [setFast] is given; [refusal]: why the agent turned it down): one card, none when there's nothing to set.
 */
@Composable private fun SettingsCard(catalog: AgentModels?, model: String?, options: Map<String, String>, setOptions: (Map<String, String>) -> Unit,
    approvals: BotDraft?, setApprovals: (BotDraft) -> Unit, setFast: ((Boolean) -> Unit)?, fast: Boolean, refusal: String?) {
    val settings: List<ModelOption> = catalog.settings(model)
    val error = catalog.selected(model)?.optionsError
    if (settings.isEmpty() && error == null && approvals == null && setFast == null) return
    Column(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp).fillMaxWidth().clip(RoundedRectangle(20.dp)).background(Palette.surface)
        .padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        for (option in settings) {
            if (option.isReasoning) {
                ReasoningSlider(option, options[option.id]) { level ->
                    setOptions(if (level == null) options - option.id else options + (option.id to level))
                }
            } else SettingLine(option.name) {
                ChoicePicker(options[option.id] ?: option.default.orEmpty(), option.values.map { it.id to it.name },
                    { setOptions(options + (option.id to it)) }, label = option.name)
            }
        }
        error?.let { Text(it, style = Type.caption, color = Palette.warning) }
        if (approvals != null) {
            val (all, current) = ApprovalChoice.choices(approvals, catalog)
            SettingLine("Approvals") {
                ChoicePicker(current.id, all.map { it.id to it.label }, { setApprovals(approvals.withApproval(it)) }, label = "Approvals")
            }
        }
        if (setFast != null) {
            SettingLine("Speed") {
                Row(Modifier.clip(Capsule()).background(if (fast) Palette.text else Palette.background)
                    .clickable(role = Role.Button) { setFast(!fast) }.semantics { this.selected = fast }
                    .padding(horizontal = 12.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    val ink = if (fast) Palette.background else Palette.secondary
                    Symbol("bolt", size = 12.dp, tint = ink)
                    Text(tr("Fast"), style = Type.subheadline.copy(fontWeight = FontWeight.Medium), color = ink)
                }
            }
            refusal?.let { Text(tr(it), style = Type.caption, color = Palette.secondary) }
        }
    }
}

/** A setting in the card: its [label], then its [value] at the end. */
@Composable private fun SettingLine(label: String, value: @Composable RowScope.() -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 32.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(tr(label), Modifier.weight(1f), style = Type.subheadline, color = Palette.secondary)
        value()
    }
}
