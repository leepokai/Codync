package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.Routine
import com.codync.android.core.routineOwners
import com.codync.android.core.routineSections
import com.codync.android.design.*
import kotlinx.coroutines.delay

/** The open editor: a bot's routine (its id), or "" setting up a new one for that bot. */
private data class RoutineTarget(val botId: String, val id: String)

/**
 * The account menu's Routines (iOS `AllRoutinesView`): every bot's routines, a section per bot, each with its switch;
 * + sets one up for a bot, first asking which when there's more than one.
 */
@Composable internal fun AllRoutinesView(state: AppState, store: AppStore) {
    val online = state.actualConnection is LinkState.Ready
    val all = state.allRoutines
    var picking by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<RoutineTarget?>(null) }
    val shown = roster(state.bots)
    val owners = routineOwners(shown)
    LaunchedEffect(online) { if (online) while (true) { store.loadAllRoutines(); delay(3_000) } }
    fun open(botId: String, id: String) { picking = false; editing = RoutineTarget(botId, id) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(if (picking) "Which bot runs it?" else "Routines") {
            if (picking) IconButton("Back", "chevron.left") { picking = false }
            else IconButton("Set up a routine", "plus", enabled = online && owners.isNotEmpty()) {
                if (owners.size == 1) open(owners.single().id, "") else picking = true
            }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, bottom = 20.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (picking) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                for (bot in owners) BotLine(bot, state.bots, 24.dp, Palette.text,
                    Modifier.outlinedCard(vertical = 0.dp).plainButton { open(bot.id, "") }.padding(vertical = 12.dp))
            } else {
                val hidden = state.bots.filter { it.hidden && !it.deleted }.sortedBy { it.name }
                val sections = routineSections(all.orEmpty(), shown + hidden)
                if (sections.isEmpty()) Text(tr(if (all != null) "No routines yet. Set one up for a bot with +." else "Loading routines…"),
                    Modifier.outlinedCard(vertical = 14.dp), style = Type.system(12f).copy(lineHeight = Type.system(12f).lineHeight * 1.25f), color = Palette.secondary)
                else for (section in sections) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    BotLine(section.bot, state.bots, 20.dp, Palette.secondary, Modifier.semantics { heading() })
                    for (routine in section.routines) {
                        RoutineRow(routine, summary(routine), online && "routines:${routine.botId}" !in state.busy, { open(routine.botId, routine.id) }) {
                            store.routineAction(routine.botId, routine.id, "setRoutineEnabled", it)
                        }
                    }
                }
            }
        }
    }
    // A routine's editor opens once that routine is listed (as in a bot's own list).
    CodyncSheet(editing?.takeIf { target -> target.id.isEmpty() || all.orEmpty().any { it.id == target.id } }, { editing = null }) { target ->
        AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Routines))
        RoutineEditor(target.botId, all?.firstOrNull { it.id == target.id }, state, store) { saved ->
            // A new webhook routine stays open: its URL and key exist only now.
            editing = if (target.id.isEmpty() && saved?.hasWebhook == true) target.copy(id = saved.id) else null
        }
    }
}

/** The row's second line: why its schedule needs a look, else when it runs. */
private fun summary(routine: Routine): String =
    if (routine.lastError != null) "Schedule needs attention" else routine.triggerDescriptions.joinToString(" · ")

/** A bot's face and name on one line: a section's heading, or a choice in "Which bot runs it?". */
@Composable private fun BotLine(bot: Bot, bots: List<Bot>, size: Dp, color: Color, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        BotAvatar(bot, bots, size)
        Text(bot.name, style = if (size < 24.dp) Type.system(13f).semibold else Type.body, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}
