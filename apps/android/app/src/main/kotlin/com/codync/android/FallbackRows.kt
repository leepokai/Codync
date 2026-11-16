package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentStep
import com.codync.android.core.Bot
import com.codync.android.core.Fallback
import com.codync.android.core.Login
import com.codync.android.core.MAX_FALLBACKS
import com.codync.android.core.Resting
import com.codync.android.core.freeLogins
import com.codync.android.core.login
import com.codync.android.core.moved
import com.codync.android.core.restOf
import com.codync.android.core.stepBanner
import com.codync.android.design.*
import com.kyant.shapes.Capsule

// A bot's fallbacks in its Agent card: the agents that carry on, in order, when its own can't answer.

private const val ABOUT_FALLBACKS = "When this bot's agent runs out of usage, is rate limited, signed out or won't start, the next " +
    "agent here carries on in the same chat, with the same memory and workspace. The bot goes back to its own agent when it " +
    "recovers. A fallback that uses an API key costs money. To fall back between one agent's models, set that in the agent " +
    "itself (Claude Code's fallbackModel)."

/** The computer's agents as the Agent picker lists them: id and name, "(not installed)" when it isn't. */
internal fun agentChoices(state: AppState): List<Pair<String, String>> = backends(state).map { info ->
    val id = info.text("id").orEmpty()
    id to ((info.text("name") ?: id) + if (!info.available) " (not installed)" else "")
}

/** The chain's logins, the bot's [own] first, without step [but]'s: each is in it once, and the step being edited keeps its own. */
private fun taken(own: Login, list: List<Fallback>, but: Int): List<Login> = listOf(own) + list.filterIndexed { i, _ -> i != but }.map { it.login }

/** The agents a step can use: each that still has a login the rest of the chain ([taken]) doesn't, and the step's [own] (its
 *  login may have signed out since). */
private fun fallbackChoices(state: AppState, taken: List<Login>, own: String? = null): List<Pair<String, String>> {
    val free = agentBackends(state).filter { it.id == own || it.freeLogins(taken).isNotEmpty() }.map { it.id }
    return agentChoices(state).filter { it.first in free }
}

/** The Agent card's first row while the bot runs on a fallback ([step]): which one, until when, and Switch back. */
@Composable internal fun FallbackBanner(state: AppState, store: AppStore, bot: Bot, step: AgentStep) {
    val unnamed = tr("New login profile")
    val default = tr("Default login profile")
    // "Claude Code (Work)": a login on a login profile, and the computer's own when it's asked for by name.
    val name = { login: Login, own: Boolean ->
        val backend = agentBackends(state).firstOrNull { it.id == login.backend }
        val loginProfile = login.loginProfile?.let { id -> backend?.loginProfiles?.firstOrNull { it.id == id }?.name ?: unnamed }
            ?: if (own) backend?.defaultLoginProfile?.name ?: default else null
        backendName(state, login.backend) + if (loginProfile != null) " ($loginProfile)" else ""
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        AgentIcon(agentRegistry(state, step.agent), 22.dp)
        Text(bot.stepBanner(name, RelativeTime::clock).orEmpty(), Modifier.weight(1f), style = Type.subheadline, color = Palette.text)
        if ("switchBack:" + bot.id in state.busy) Spinner() else SecondaryButton("Switch back") { store.switchBack(bot) }
    }
}

/** The Fallbacks header (ⓘ and "+"), a row per fallback, and what they do. Changes go into the form like any other. */
@Composable internal fun FallbackList(state: AppState, store: AppStore, own: Login, list: List<Fallback>, bot: Bot?, change: (List<Fallback>) -> Unit,
    modifier: Modifier = Modifier) {
    var about by remember { mutableStateOf(false) }
    // The step the sheet is open on: its place, and its own as the sheet opened (null for a new one, at the end).
    var sheet by remember { mutableStateOf<Pair<Int, Fallback?>?>(null) }
    Column(modifier.fillMaxWidth().animateContentSize(Motion.layoutSize), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(tr("Fallbacks"), style = Type.body, color = Palette.text)
            IconButton("About fallbacks", "info.circle") { about = true }
            Box(Modifier.weight(1f))
            if (list.size < MAX_FALLBACKS && fallbackChoices(state, taken(own, list, list.size)).isNotEmpty()) IconButton("Add fallback", "plus") { sheet = list.size to null }
        }
        if (list.isEmpty()) Text(tr("No fallbacks yet."), style = Type.subheadline, color = Palette.secondary)
        for ((index, fallback) in list.withIndex()) key(fallback.login) {
            FallbackRow(state, index + 1, fallback, bot?.restOf(fallback.login), bot?.agentStep?.login == fallback.login) { buildList {
                if (index > 0) add(MenuItem("Move up", icon = "arrow.up") { change(list.moved(index, -1)) })
                if (index < list.lastIndex) add(MenuItem("Move down", icon = "arrow.down") { change(list.moved(index, 1)) })
                add(MenuItem("Edit", icon = "pencil") { sheet = index to fallback })
                add(MenuItem("Remove", icon = "trash", destructive = true, divider = true) { change(list - fallback) })
            } }
        }
        Text(tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.")
            .replace("{agent}", backendName(state, own.backend)), style = Type.caption, color = Palette.secondary)
    }
    CodyncDialog("Fallbacks", about, { about = false }, ABOUT_FALLBACKS, cancel = "OK") { emptyList() }
    CodyncSheet(sheet, { sheet = null }) { (i, initial) ->
        FallbackSheet(state, store, taken(own, list, i), initial) { new ->
            change(list.toMutableList().apply { if (initial != null && i < size) set(i, new) else add(new) }); sheet = null
        }
    }
}

@Composable private fun FallbackRow(state: AppState, number: Int, fallback: Fallback, rest: Resting?, inUse: Boolean, menu: () -> List<MenuItem>) {
    val agent = tr(agentChoices(state).firstOrNull { it.first == fallback.backend }?.second ?: backendName(state, fallback.backend))
    val loginProfile = fallback.loginProfile?.let { id -> agentBackends(state).firstOrNull { it.id == fallback.backend }?.loginProfiles?.firstOrNull { it.id == id } }
    val name = if (loginProfile != null) "$agent (${loginProfile.name ?: tr("New login profile")})" else agent
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("$number", style = Type.subheadline.monospacedDigit, color = Palette.secondary)
        AgentIcon(agentRegistry(state, fallback.backend), 22.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            // ponytail: the model's id, not its name: a name needs the agent's model list, an agent start per row.
            Text("$name · ${tr(fallback.model ?: "Default")}", style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (rest != null) Text(rest.line(RelativeTime.clock(rest.until)), style = Type.caption, color = Palette.secondary)
        }
        if (inUse) Text("In use", Modifier.clip(Capsule()).background(Palette.accentFill).padding(horizontal = 8.dp, vertical = 3.dp),
            style = Type.caption.medium, color = Palette.onAccent)
        DropdownMenu(menu, label = "$name actions") {
            Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Symbol("ellipsis", size = 18.dp, tint = Palette.secondary) }
        }
    }
}

/** "+" (or a row's Edit, with the step's own as [initial]): an agent (or the bot's own on another of its login profiles) and its
 *  model. [chain] is the rest of the chain's logins. */
@Composable private fun FallbackSheet(state: AppState, store: AppStore, chain: List<Login>, initial: Fallback?, save: (Fallback) -> Unit) {
    // Taken as the sheet opens, so the added login doesn't drop out of the pickers while the sheet slides away.
    val taken = remember { chain }
    val choices = remember { fallbackChoices(state, taken, initial?.backend) }
    // An installed agent first, as on the other apps.
    val installed = remember { agentBackends(state).filter { it.available }.map { it.id } }
    var backend by remember { mutableStateOf(initial?.backend ?: (choices.firstOrNull { it.first in installed } ?: choices.firstOrNull())?.first.orEmpty()) }
    // The step's own while its agent stays the same.
    val own = initial?.takeIf { it.backend == backend }
    var model by remember(backend) { mutableStateOf(own?.model) }
    var options by remember(backend) { mutableStateOf(own?.modelOptions.orEmpty()) }
    val agent = agentBackends(state).firstOrNull { it.id == backend }
    // A step keeps its own login, even one that has signed out since.
    val free = agent?.freeLogins(taken).orEmpty().let { if (own == null || own.loginProfile in it) it else it + own.loginProfile }
    // The first login the chain doesn't have yet, until the user picks one.
    var picked by remember(backend) { mutableStateOf(own?.loginProfile) }
    val loginProfile = if (picked in free) picked else free.firstOrNull()
    Column(Modifier.fillMaxSize()) {
        ModalHeader(if (initial != null) "Edit fallback" else "Add fallback") {
            IconButton(if (initial != null) "Save" else "Add fallback", "checkmark", enabled = backend.isNotEmpty()) {
                save(Fallback(backend, command = own?.command, model = model, modelOptions = options, loginProfile = loginProfile))
            }
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp)) {
            CardSection {
                row { AgentModelPicker(state, store::agentModels, backend, model, options, false, null, { id, picked -> model = id; options = picked }, null, agents = choices, chooseAgent = { backend = it }, collapsible = false) }
                if (agent?.loginProfiles != null) row { LoginProfilePicker(agent, loginProfile, state, store, free) { picked = it } }
            }
        }
    }
}
