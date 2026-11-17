package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentStep
import com.codync.android.core.Bot
import com.codync.android.core.Fallback
import com.codync.android.core.Login
import com.codync.android.core.MAX_FALLBACKS
import com.codync.android.core.fill
import com.codync.android.core.login
import com.codync.android.core.moved
import com.codync.android.core.restOf
import com.codync.android.core.stepBannerPhrase
import com.codync.android.core.stepLogins
import com.codync.android.core.stepOn
import com.codync.android.core.usable
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

/** The chain's logins, the bot's [own] first, without step [but]'s: each is in it once, and step [but] keeps its own. */
private fun taken(own: Login, list: List<Fallback>, but: Int): List<Login> = listOf(own) + list.filterIndexed { i, _ -> i != but }.map { it.login }

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
        // The phrase is translated before its names go in: a login profile named "Work while traveling" stays whole.
        val banner = bot.stepBannerPhrase(name, RelativeTime::clock)
        Text(banner?.let { fill(tr(it.key), it.values) }.orEmpty(), Modifier.weight(1f), style = Type.subheadline, color = Palette.text)
        if ("switchBack:" + bot.id in state.busy) Spinner() else SecondaryButton("Switch back") { store.switchBack(bot) }
    }
}

/** The Fallbacks header (ⓘ and "+"), a row per fallback picked in place like the bot's own agent, and what they do. Changes
 *  go into the form like any other. */
@Composable internal fun FallbackList(state: AppState, store: AppStore, own: Login, list: List<Fallback>, bot: Bot?, change: (List<Fallback>) -> Unit,
    modifier: Modifier = Modifier) {
    var about by remember { mutableStateOf(false) }
    // The step "+" just added: it opens on its pickers.
    var added by remember { mutableStateOf<Int?>(null) }
    // Bumped when a move or removal renumbers the rows, so each starts afresh on the step now in its place.
    var order by remember { mutableIntStateOf(0) }
    fun renumber(next: List<Fallback>) { added = null; order++; change(next) }
    Column(modifier.fillMaxWidth().animateContentSize(Motion.layoutSize), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(tr("Fallbacks"), style = Type.body, color = Palette.text)
            IconButton("About fallbacks", "info.circle") { about = true }
            Box(Modifier.weight(1f))
            val chain = taken(own, list, list.size)
            // An installed agent first, as on the other apps.
            val agent = agentBackends(state).usable(chain).let { free -> free.firstOrNull { it.available } ?: free.firstOrNull() }
            if (list.size < MAX_FALLBACKS && agent != null) IconButton("Add fallback", "plus") {
                added = list.size
                change(list + agent.stepOn(chain))
            }
        }
        if (list.isEmpty()) Text(tr("No fallbacks yet."), style = Type.subheadline, color = Palette.secondary)
        // By place, not login: picking another agent keeps the row (and its open picker) where it is.
        for ((index, fallback) in list.withIndex()) key(order, index) {
            FallbackRow(state, store, index + 1, fallback, taken(own, list, index), bot, added == index,
                { change(list.toMutableList().apply { set(index, it) }) }) { buildList {
                if (index > 0) add(MenuItem("Move up", icon = "arrow.up") { renumber(list.moved(index, -1)) })
                if (index < list.lastIndex) add(MenuItem("Move down", icon = "arrow.down") { renumber(list.moved(index, 1)) })
                add(MenuItem("Remove", icon = "trash", destructive = true, divider = true) { renumber(list - fallback) })
            } }
        }
        Text(tr("If {agent} runs out of usage or can't answer, the bot carries on with the next one. Chat, memory and workspace carry over.")
            .replace("{agent}", backendName(state, own.backend)), style = Type.caption, color = Palette.secondary)
    }
    CodyncDialog("Fallbacks", about, { about = false }, ABOUT_FALLBACKS, cancel = "OK") { emptyList() }
}

/** One fallback: its place, its agent, model and login picked in place ([taken] is the rest of the chain's logins), whether
 *  it's resting or in use, and its menu. */
@Composable private fun FallbackRow(state: AppState, store: AppStore, number: Int, fallback: Fallback, taken: List<Login>, bot: Bot?, expanded: Boolean,
    edit: (Fallback) -> Unit, menu: () -> List<MenuItem>) {
    // The step as it was on its own agent while another is picked: picking that agent again brings back its model and login.
    var own by remember { mutableStateOf<Fallback?>(null) }
    val choices = agentBackends(state).usable(taken, listOfNotNull(fallback.login, own?.login))
    val agent = choices.firstOrNull { it.id == fallback.backend }
    val loginProfile = fallback.loginProfile?.let { id -> agent?.loginProfiles?.firstOrNull { it.id == id } }
    // The step's login shows beside its agent: "Claude Code (Work)".
    val agents = agentChoices(state).filter { (id, _) -> choices.any { it.id == id } }.map { (id, label) ->
        id to if (id == fallback.backend && loginProfile != null) "${tr(label)} (${loginProfile.name ?: tr("New login profile")})" else label
    }
    val name = agents.firstOrNull { it.first == fallback.backend }?.second ?: backendName(state, fallback.backend)
    val rest = bot?.restOf(fallback.login)
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            // On the summary line (its 8dp padding).
            Text("$number", Modifier.padding(top = 8.dp), style = Type.subheadline.monospacedDigit, color = Palette.secondary)
            Box(Modifier.weight(1f)) {
                AgentModelPicker(state, store::agentModels, fallback.backend, fallback.model, fallback.modelOptions, false, null,
                    { model, options -> edit(fallback.copy(model = model, modelOptions = options)) }, null, agents = agents,
                    chooseAgent = { id ->
                        choices.firstOrNull { it.id == id }?.let { next ->
                            val step = next.stepOn(taken, own)
                            own = if (step == own) null else own ?: fallback
                            edit(step)
                        }
                    }, expanded = expanded) {
                    if (agent?.loginProfiles != null) LoginProfilePicker(agent, fallback.loginProfile, state, store, agent.stepLogins(fallback.login, taken)) {
                        edit(fallback.copy(loginProfile = it))
                    }
                }
            }
            if (bot?.agentStep?.login == fallback.login) Text("In use", Modifier.padding(top = 6.dp).clip(Capsule()).background(Palette.accentFill)
                .padding(horizontal = 8.dp, vertical = 3.dp), style = Type.caption.medium, color = Palette.onAccent)
            DropdownMenu(menu, label = "$name actions") {
                Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Symbol("ellipsis", size = 18.dp, tint = Palette.secondary) }
            }
        }
        if (rest != null) Text(tr(rest.line(RelativeTime.clock(rest.until))), style = Type.caption, color = Palette.secondary)
    }
}
