package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.Fallback
import com.codync.android.core.restOf
import com.codync.android.design.*

/**
 * "Advisor mode", only from a host that has it: the switch, and while it's on the threads' own agent and fallbacks, which
 * unfold under it split by hairlines like rows of their own.
 */
internal fun CardRows.advisorRow(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit, bot: Bot?) {
    if (draft.threadFallbacks == null) return
    row {
        // The threads' rows keep their last setup while they fold away.
        var shown by remember { mutableStateOf(draft) }
        if (draft.threadAgent != null && draft != shown) shown = draft
        Column {
            SwitchRow("Advisor mode", "The chat plans and hands work on; threads run on an agent of their own, with its own fallbacks.",
                draft.threadAgent != null, { change(draft.withAdvisor(it, agentBackends(state))) })
            AnimatedVisibility(draft.threadAgent != null, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
                exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
                val agent = shown.threadAgent ?: return@AnimatedVisibility
                // While they fold away, they take no edits.
                val edit = { next: BotDraft -> if (draft.threadAgent != null) change(next) }
                // A row's padding on each side of its hairline, as the card draws them.
                Column(Modifier.padding(top = 14.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Hairline()
                    ThreadsAgent(state, store, agent, bot, { edit(draft.copy(threadAgent = it)) }) { edit(draft.withThreadAgent(it)) }
                    Hairline()
                    FallbackList(state, store, agent.login, shown.threadFallbacks.orEmpty(), bot, { edit(draft.copy(threadFallbacks = it)) }, threads = true)
                }
            }
        }
    }
}

/** Advisor mode's agent for the bot's threads: picked in place with its model and login, as the bot's own, and its rest. */
@Composable private fun ThreadsAgent(state: AppState, store: AppStore, agent: Fallback, bot: Bot?, edit: (Fallback) -> Unit, chooseAgent: (String) -> Unit) {
    val backend = agentBackends(state).firstOrNull { it.id == agent.backend }
    val rest = bot?.restOf(agent.login)
    Field("Threads' agent") {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            AgentModelPicker(state, store::agentModels, agent.backend, agent.model, agent.modelOptions, false, null,
                { model, options -> edit(agent.copy(model = model, modelOptions = options)) }, null, agents = agentChoices(state), chooseAgent = chooseAgent) {
                if (backend?.loginProfiles != null) LoginProfilePicker(backend, agent.loginProfile, state, store) { edit(agent.copy(loginProfile = it)) }
            }
            if (rest != null) Text(tr(rest.line(RelativeTime.clock(rest.until))), style = Type.caption, color = Palette.secondary)
        }
    }
}
