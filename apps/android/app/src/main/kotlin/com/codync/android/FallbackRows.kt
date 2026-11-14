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

/** The agents a new fallback can use: each that still has a login the chain ([taken]: the bot's own and its fallbacks') doesn't. */
private fun fallbackChoices(state: AppState, taken: List<Login>): List<Pair<String, String>> {
    val free = agentBackends(state).filter { it.freeLogins(taken).isNotEmpty() }.map { it.id }
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
@Composable internal fun FallbackList(state: AppState, own: Login, list: List<Fallback>, bot: Bot?, change: (List<Fallback>) -> Unit,
    addFallback: () -> Unit, modifier: Modifier = Modifier) {
    var about by remember { mutableStateOf(false) }
    Column(modifier.fillMaxWidth().animateContentSize(Motion.layoutSize), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(tr("Fallbacks"), style = Type.body, color = Palette.text)
            IconButton("About fallbacks", "info.circle") { about = true }
            Box(Modifier.weight(1f))
            if (list.size < MAX_FALLBACKS && fallbackChoices(state, listOf(own) + list.map { it.login }).isNotEmpty()) IconButton("Add fallback", "plus", action = addFallback)
        }
        if (list.isEmpty()) Text(tr("No fallbacks yet."), style = Type.subheadline, color = Palette.secondary)
        for ((index, fallback) in list.withIndex()) key(fallback.login) {
            FallbackRow(state, index + 1, fallback, bot?.restOf(fallback.login), bot?.agentStep?.login == fallback.login) { buildList {
                if (index > 0) add(MenuItem("Move up", icon = "arrow.up") { change(list.moved(index, -1)) })
                if (index < list.lastIndex) add(MenuItem("Move down", icon = "arrow.down") { change(list.moved(index, 1)) })
                add(MenuItem("Remove", icon = "trash", destructive = true) { change(list - fallback) })
            } }
        }
        Text(tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.")
            .replace("{agent}", backendName(state, own.backend)), style = Type.caption, color = Palette.secondary)
    }
    CodyncDialog("Fallbacks", about, { about = false }, ABOUT_FALLBACKS, cancel = "OK") { emptyList() }
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

/** "+": another agent (or the bot's own on another of its login profiles) and its model, added at the end of the list. */
@Composable internal fun AddFallbackSheet(state: AppState, store: AppStore, chain: List<Login>, add: (Fallback) -> Unit) {
    // Taken as the sheet opens, so the added login doesn't drop out of the pickers while the sheet slides away.
    val taken = remember { chain }
    val choices = remember { fallbackChoices(state, taken) }
    // An installed agent first, as on the other apps.
    val installed = remember { agentBackends(state).filter { it.available }.map { it.id } }
    var backend by remember { mutableStateOf((choices.firstOrNull { it.first in installed } ?: choices.firstOrNull())?.first.orEmpty()) }
    var model by remember(backend) { mutableStateOf<String?>(null) }
    var options by remember(backend) { mutableStateOf(emptyMap<String, String>()) }
    val agent = agentBackends(state).firstOrNull { it.id == backend }
    val free = agent?.freeLogins(taken).orEmpty()
    // The first login the chain doesn't have yet, until the user picks one.
    var picked by remember(backend) { mutableStateOf<String?>(null) }
    val loginProfile = if (picked in free) picked else free.firstOrNull()
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Add fallback") {
            IconButton("Add fallback", "checkmark", enabled = backend.isNotEmpty()) { add(Fallback(backend, model = model, modelOptions = options, loginProfile = loginProfile)) }
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp)) {
            CardSection {
                row { AgentModelPicker(state, store::agentModels, backend, model, options, false, null, { id, picked -> model = id; options = picked }, null, agents = choices, chooseAgent = { backend = it }) }
                if (agent?.loginProfiles != null) row { LoginProfilePicker(agent, loginProfile, state, store, free) { picked = it } }
            }
        }
    }
}
